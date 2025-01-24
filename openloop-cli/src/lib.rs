use std::time::Duration;

use tokio::{
    process::Command,
    sync::mpsc::Sender,
    task::JoinSet,
    time::{self, sleep},
};

pub async fn open_loop_generator<G>(
    get_command_line: G,
    qps: usize,
    request_num: usize,
    clean_chan: Sender<String>,
) -> Vec<f64>
where
    G: FnOnce() -> (String, Vec<String>, String) + Send + Copy + 'static,
{
    let mut interval = time::interval(Duration::from_millis((1000. / qps as f64) as u64));
    let mut q_counter = 0;
    let mut join_set = JoinSet::new();

    loop {
        let clean_chan = clean_chan.clone();
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

    join_set.join_all().await;
    sleep(Duration::from_millis(200)).await;

    vec![]
}
