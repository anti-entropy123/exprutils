use std::sync::atomic::{AtomicU32, Ordering};

use tokio::{process::Command, sync::mpsc::Sender, task};

pub async fn close_loop_generator<G>(
    get_command_line: G,
    concur_num: usize,
    total_task_num: usize,
    cleanup_chan: Sender<String>,
) -> Vec<f64>
where
    G: FnOnce() -> (String, Vec<String>, String) + Send + Copy + 'static,
{
    // let (tx, mut rx) = mpsc::channel(concur_num);
    // let cmd_lists: Arc<Vec<_>> = Arc::new(cmd_lists.iter().map(|s| s.to_string()).collect());
    static TASK_NUM: AtomicU32 = AtomicU32::new(0);
    TASK_NUM.store(0, Ordering::SeqCst);

    let mut handles = vec![];

    for _ in 0..concur_num {
        // let tx = tx.clone();
        // let regex_str = regex_str.to_owned();
        let id_sender = cleanup_chan.clone();
        // let cmd_lists = Arc::clone(&cmd_lists);
        let handle = task::spawn(async move {
            loop {
                if TASK_NUM.fetch_add(1, Ordering::AcqRel) >= total_task_num as u32 {
                    break;
                }
                // let cmd_lists: Vec<String> = cmd_lists.iter().map(|s| ph2random(s)).collect();
                let (cmd, args, id) = get_command_line();
                let output = Command::new(&cmd)
                    .args(&args)
                    .output()
                    .await
                    .expect("Failed to execute command");

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let output = format!("{stderr}\n{stdout}");
                println!("output: {}", output);
                // if !stderr.is_empty() {
                //     eprintln!("Error: {}", stderr);
                // }

                // let regex = Regex::new(&regex_str).unwrap();
                // let mut durations = vec![];

                // for line in output.lines() {
                //     if let Some(captures) = regex.captures(line) {
                //         if let Some(duration_str) = captures.get(1) {
                //             if let Ok(duration) = duration_str.as_str().parse::<f64>() {
                //                 // if !line.ends_with("ms") {
                //                 //     duration *= 1000.;
                //                 // }
                //                 durations.push(duration);
                //             }
                //             continue;
                //         }
                //     }
                // }
                // for duration in durations {
                //     let _ = tx.send(duration).await;
                // }

                if !id.is_empty() {
                    id_sender.send(id).await.unwrap();
                };
            }
        });

        handles.push(handle);
    }

    // drop(tx);

    // let mut latencies = vec![];
    // while let Some(latency) = rx.recv().await {
    //     latencies.push(latency);
    // }

    for handle in handles {
        let _ = handle.await;
    }

    Default::default()
}
