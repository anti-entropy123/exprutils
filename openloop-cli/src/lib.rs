use std::time::Duration;

use regex::Regex;
use tokio::{
    process::Command,
    sync::mpsc::{self, Sender},
    task::JoinSet,
    time::{self, sleep},
};

pub async fn open_loop_generator<G>(
    get_command_line: G,
    qps: usize,
    request_num: usize,
    clean_chan: Sender<String>,
    regex_str: &str,
) -> Vec<f64>
where
    G: FnOnce() -> (String, Vec<String>, String) + Send + Copy + 'static,
{
    let (tx, mut rx) = mpsc::channel(qps);
    let mut interval = time::interval(Duration::from_millis((1000. / qps as f64) as u64));
    let mut q_counter = 0;
    let mut join_set = JoinSet::new();

    let mut latencies = vec![];

    loop {
        let clean_chan = clean_chan.clone();
        interval.tick().await;
        let tx = tx.clone();
        let regex_str = regex_str.to_owned();
        join_set.spawn(async move {
            let (cmd, args, id) = get_command_line();
            println!("{} {}", cmd, args.join(" "));
            let output = match Command::new(cmd).args(&args).output().await {
                Ok(output) => {
                    println!("Command executed successfully.");
                    // println!("Output: {}", String::from_utf8_lossy(&output.stdout));
                    output
                }
                Err(e) => {
                    eprintln!("Failed to execute command: {}", e);
                    panic!("{}", e);
                }
            };

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let output = format!("{stderr}\n{stdout}");
            // println!("output: {}", output);

            let regex = Regex::new(&regex_str).unwrap();
            let mut durations = vec![];

            for line in output.lines() {
                if let Some(captures) = regex.captures(line) {
                    if let Some(duration_str) = captures.get(1) {
                        if let Ok(duration) = duration_str.as_str().parse::<f64>() {
                            // if !line.ends_with("ms") {
                            //     duration *= 1000.;
                            // }
                            durations.push(duration);
                            break;
                        }
                        continue;
                    }
                }
            }
            
            for duration in durations {
                let _ = tx.send(duration).await;
            }

            if !id.is_empty() {
                // cleanup(id).await;
                clean_chan.send(id).await.unwrap()
            }
        });

        q_counter += 1;
        if q_counter == request_num {
            break;
        }
    }

    drop(tx);

    while let Some(latency) = rx.recv().await {
        latencies.push(latency);
    }

    join_set.join_all().await;
    sleep(Duration::from_millis(200)).await;

    latencies
}
