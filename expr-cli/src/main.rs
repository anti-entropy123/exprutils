use nix::sys::signal;
use rand::Rng;
use std::time::Duration;
use tokio::{
    fs::{File, OpenOptions},
    io::AsyncWriteExt,
    process::Command,
    sync::mpsc::{self, Receiver},
    time::{self, sleep},
};

const _PLACEHOLDER: &str = "xyz";

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

fn _ph2random(s: &str) -> String {
    if s != _PLACEHOLDER {
        return s.to_owned();
    }

    random_id()
}

fn get_command_line() -> (String, Vec<String>, String) {
    ///////////// as map_reduce c5 //////////////////
    // let command = "target/release/asvisor".to_owned();
    // let args = vec![
    //     "--files".to_owned(),
    //     "isol_config/map_reduce_large_c5.json".to_owned(),
    // ];
    // let id = String::new();

    ///////////// as parallel_sort c3 //////////////////
    let command = "target/release/asvisor".to_owned();
    let args = vec![
        "--files".to_owned(),
        "isol_config/parallel_sort_c3.json".to_owned(),
        "--metrics".to_owned(),
        "total-dur".to_owned(),
    ];
    let id = String::new();

    ///////////// as parallel_sort c5 //////////////////
    // let command = "target/release/asvisor".to_owned();
    // let args = vec![
    //     "--files".to_owned(),
    //     "isol_config/parallel_sort_c5.json".to_owned(),
    //     "--metrics".to_owned(),
    //     "total-dur".to_owned(),
    // ];
    // let id = String::new();

    ///////////// faastlane map_reduce c5 //////////
    // let command = "ctr".to_owned();
    // let id = random_id();
    // let args: Vec<String> = vec![
    //     "run".to_owned(),
    //     "--snapshotter".to_owned(),
    //     "devmapper".to_owned(),
    //     "--runtime".to_owned(),
    //     "io.containerd.kata-fc.v2".to_owned(),
    //     "--rm".to_owned(),
    //     "docker.io/library/word_count_5_10_asstlane_image:latest".to_owned(),
    //     id.clone(),
    //     "target/release/word_count".to_owned(),
    // ];

    /////////// faastlane parallel_sort c5 //////////
    // let command = "ctr".to_owned();
    // let id = random_id();
    // let args: Vec<String> = vec![
    //     "run".to_owned(),
    //     "--snapshotter".to_owned(),
    //     "devmapper".to_owned(),
    //     "--runtime".to_owned(),
    //     "io.containerd.kata-fc.v2".to_owned(),
    //     "--rm".to_owned(),
    //     "docker.io/library/parallel_sort_5_25_asstlane_image:latest".to_owned(),
    //     id.clone(),
    //     "target/release/parallel_sort".to_owned(),
    // ];

    (command, args, id)
}

#[tokio::main]
async fn main() {
    for _ in 0..1 {
        test_once().await
    }
}

async fn test_once() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <concur_num> <command> <regex>", args[0]);
        std::process::exit(1);
    }

    let concur_num: usize = args[1]
        .parse()
        .expect("Please provide a valid number for concur_num");

    let regex_str = if let Some(regex) = args.get(3) {
        regex
    } else {
        r#""total_dur\(ms\)": ([\d.]+)"#
    };

    // let cmd_list: Vec<&str> = args[2].split_ascii_whitespace().collect();

    let (id_sender, id_receiver) = mpsc::channel(10);
    let (stop_ch_tx, stop_ch_rx) = mpsc::channel(1);

    let log_file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open("./monitor.log")
        .await
        .expect("open log file failed");
    log_file.set_len(0).await.unwrap();

    let monitor_worker = tokio::spawn(monitor_resource(log_file, stop_ch_rx));
    let cleanup_worker = tokio::spawn(firecracker_worker(id_receiver));

    // let total_task_num = concur_num * 15 + 1;
    // let latencies = closeloop_cli::close_loop_generator(
    //     get_command_line,
    //     concur_num,
    //     total_task_num,
    //     id_sender,
    // )
    // .await;

    let total_task_num = concur_num * 10 + 1;
    let latencies = openloop_cli::open_loop_generator(
        get_command_line,
        concur_num,
        total_task_num,
        id_sender,
        regex_str,
    )
    .await;

    drop(stop_ch_tx);

    cleanup_worker.await.unwrap();
    sleep(Duration::from_secs(1)).await;

    monitor_worker.await.unwrap();

    if latencies.is_empty() {
        println!("No latencies were recorded.");
    } else {
        let mut sorted_latencies = latencies;
        sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "Latencies len = {}: {:?}",
            sorted_latencies.len(),
            sorted_latencies
        );
        if sorted_latencies.len() > 1 {
            println!("p99: {}", sorted_latencies[sorted_latencies.len() - 2]);
        }
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

async fn monitor_resource(mut log_file: File, stop_chan: Receiver<bool>) {
    let mut mems: Vec<usize> = vec![];
    let interval = Duration::from_millis(100); // 每 100 毫秒采集一次

    let mut interval_timer = time::interval(interval);

    loop {
        if stop_chan.is_closed() {
            break;
        }
        interval_timer.tick().await;

        // 获取当前时间戳
        let timestamp = chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S%.3f")
            .to_string();

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
        let mut mem_usage: f64 = 0.;
        if let Some(mem_line) = output_str.lines().find(|line| line.contains("MiB Mem")) {
            let parts: Vec<&str> = mem_line.split_whitespace().collect();
            if parts.len() > 7 {
                mem_usage = parts[7].parse().unwrap(); // 可用内存
            }
        }
        mems.push(mem_usage as usize);

        // let mut self_mem_mib: i32 = 0;
        // if let Ok(status) = tokio::fs::read_to_string("/proc/self/status").await {
        //     for line in status.lines() {
        //         if line.starts_with("VmRSS:") {
        //             // 提取内存值（以 KB 为单位）
        //             if let Some(mem_kb_str) = line.split_whitespace().nth(1) {
        //                 if let Ok(mem_kb) = mem_kb_str.parse::<i32>() {
        //                     // 转换为 MiB
        //                     self_mem_mib = mem_kb / 1024;
        //                     // println!("当前进程的内存使用量: {} MiB", self_mem_mib);
        //                 } else {
        //                     println!("无法解析内存值: {}", mem_kb_str);
        //                 }
        //             } else {
        //                 println!("无法提取内存值");
        //             }
        //         }
        //     }
        // } else {
        //     println!("无法读取 /proc/self/status 文件");
        // }

        // 打印结果
        println!(
            "{}, CPU: {}, Mem: {}",
            timestamp, cpu_usage, mem_usage as i32
        );

        // mems.push(mem_usage as i32 - self_mem_mib);
        log_file
            .write_all(format!("{}, {}, {}\n", timestamp, cpu_usage, mem_usage as i32).as_bytes())
            .await
            .expect("write log failed");
    }

    mems.sort();
    println!("total consume mem: {}Mib", mems.last().unwrap() - mems[0])
}
