use nix::sys::signal;
use rand::Rng;
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    process::Command,
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinSet,
    time::{self, sleep, Duration},
};

fn random_id() -> String {
    let mut rng = rand::thread_rng();
    let part1 = (0..3).map(|_| rng.gen_range(b'a'..=b'z') as char);

    let mut rng = rand::thread_rng().clone();
    let part2 = (0..6).map(|_| {
        let number = rng.gen_range(0..=9);
        char::from_u32(number + 48).unwrap()
    });

    part1.chain(part2).collect()
}

fn get_command_line() -> (String, Vec<String>, String) {
    ///////////// as map_reduce c5 //////////////////
    // let command = Arc::new("target/release/msvisor".to_string());
    // let args = Arc::new(vec![
    //     "--files".to_owned(),
    //     "isol_config/map_reduce_large_c5.json".to_owned(),
    // ]);
    // let interval_ms: u64 = (1000. / qps) as u64;

    ///////////// faastlane map_reduce c5 //////////
    let command = "ctr".to_owned();
    let id = random_id();
    let args: Vec<String> = vec![
        "run".to_owned(),
        "--snapshotter".to_owned(),
        "devmapper".to_owned(),
        "--runtime".to_owned(),
        "io.containerd.kata-fc.v2".to_owned(),
        "--rm".to_owned(),
        "docker.io/library/word_count_5_10_asstlane_image:latest".to_owned(),
        id.clone(),
        "target/release/word_count".to_owned(),
    ];

    (command, args, id)
}

#[tokio::main]
async fn main() {
    let (stop_ch_tx, stop_ch_rx) = channel(1);
    let (clean_ch_tx, clean_ch_rx) = channel(10);
    let args: Vec<String> = std::env::args().collect();
    println!("{:?}", args);

    let qps: f64 = if let Some(arg) = args.get(1) {
        arg.parse().unwrap_or(0.3)
    } else {
        0.3
    };

    let request_num: u64 = (qps * 15.) as u64;
    let interval_ms: u64 = (1000. / qps) as u64;

    let mut join_set = JoinSet::new();
    join_set.spawn(monitor_resource("monitor.log", stop_ch_rx));
    tokio::spawn(firecracker_worker(clean_ch_rx));
    // join_set.spawn(open_loop_generator(interval_ms, request_num, tx));
    join_set.spawn(openloop_cli::open_loop_generator(
        get_command_line,
        interval_ms,
        request_num,
        stop_ch_tx,
        clean_ch_tx,
    ));

    join_set.join_all().await;
}

async fn open_loop_generator(interval_ms: u64, request_num: u64, stop_chan: Sender<bool>) {
    let mut interval = time::interval(Duration::from_millis(interval_ms));
    let mut q_counter = 0;
    let mut join_set = JoinSet::new();

    loop {
        interval.tick().await;

        join_set.spawn(async move {
            let (cmd, args, id) = get_command_line();
            println!("{} {}", cmd, args.join(" "));
            match Command::new(cmd).args(&args).output().await {
                Ok(output) => {
                    println!("Command executed successfully.");
                    println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                }
                Err(e) => {
                    eprintln!("Failed to execute command: {}", e);
                }
            }
            // if !id.is_empty() {
            //     clean_firecracker(id).await;
            // }
        });

        q_counter += 1;
        if q_counter == request_num {
            break;
        }
    }

    join_set.join_all().await;
    sleep(Duration::from_millis(200)).await;
    if stop_chan.send(true).await.is_err() {
        eprintln!("Failed to send stop signal");
    }
}

async fn firecracker_worker(mut id_recv: Receiver<String>) {
    while let Some(id) = id_recv.recv().await {
        // 执行 ps 命令并获取输出
        let output = Command::new("ps")
            .arg("aux") // 或者用 "ps -ef"
            .output()
            .await
            .expect("Failed to execute ps command");

        // 将输出转为字符串
        let ps_output = String::from_utf8_lossy(&output.stdout);

        // 搜索包含特定 ID 的行
        for line in ps_output.lines() {
            if line.contains(&id) && line.contains("firecracker") {
                // 提取 PID（假设 PID 是第二列，使用空格分隔）
                if let Some(pid_str) = line.split_whitespace().nth(1) {
                    if let Ok(pid) = pid_str.parse::<i32>() {
                        println!("Found process with PID: {}", pid);

                        // 尝试杀死该进程
                        match signal::kill(
                            nix::unistd::Pid::from_raw(pid),
                            nix::sys::signal::Signal::SIGKILL,
                        ) {
                            Ok(_) => println!("Successfully killed process with PID: {}", pid),
                            Err(e) => eprintln!("Failed to kill process with PID {}: {}", pid, e),
                        }
                    }
                }
            }
        }
    }
}

async fn monitor_resource(output_file: &str, stop_chan: Receiver<bool>) {
    let interval = Duration::from_millis(100); // 每 100 毫秒采集一次

    let mut interval_timer = time::interval(interval);
    let mut log_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(output_file)
        .await
        .expect("open log file failed");

    loop {
        if stop_chan.is_closed() {
            break;
        }
        interval_timer.tick().await;

        // 获取当前时间戳
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // 异步执行 top 命令，获取输出
        let top_output = Command::new("top")
            .arg("-b")
            .arg("-n")
            .arg("1")
            .output()
            .await
            .expect("Failed to execute top command");

        // 解析输出
        let output_str = String::from_utf8_lossy(&top_output.stdout);

        // 提取 CPU 使用率
        let mut cpu_usage = String::from("N/A");
        if let Some(cpu_line) = output_str.lines().find(|line| line.starts_with("%Cpu(s):")) {
            let parts: Vec<&str> = cpu_line.split_whitespace().collect();
            if parts.len() > 3 {
                cpu_usage = format!("{}, {}", parts[1], parts[3]); // 用户和系统 CPU 使用率
            }
        }

        // 提取内存使用率
        let mut mem_usage = String::from("N/A");
        if let Some(mem_line) = output_str.lines().find(|line| line.contains("MiB Mem")) {
            let parts: Vec<&str> = mem_line.split_whitespace().collect();
            if parts.len() > 7 {
                mem_usage = parts[7].to_string(); // 可用内存
            }
        }

        // 打印结果
        println!("{}, CPU: {}, Mem: {}", timestamp, cpu_usage, mem_usage);
        log_file
            .write_all(format!("{}, {}, {}\n", timestamp, cpu_usage, mem_usage).as_bytes())
            .await
            .expect("write log failed");
    }
}
