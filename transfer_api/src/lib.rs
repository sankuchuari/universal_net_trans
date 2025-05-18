use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::broadcast;
use warp::Filter;
use warp::ws::{Message, WebSocket};
use futures_util::{StreamExt, SinkExt};
use chrono::Utc;

/// 定义各种事件
#[derive(Debug, serde::Serialize, Clone)]
pub enum TransferEvent {
    Started { file: String },
    Finished { file: String },
    Failed { file: String, err: String },
    Received { file: String },
}

/// 广播通道类型，用于发送事件到所有客户端
type EventSender = broadcast::Sender<TransferEvent>;

lazy_static::lazy_static! {
    /// 全局广播发送者
    static ref GLOBAL_SENDER: Arc<Mutex<Option<EventSender>>> = Arc::new(Mutex::new(None));
}

/// 初始化 WebSocket API 服务
pub fn init_ws_server(port: u16) {
    // 创建广播通道，缓冲 16 条消息
    let (tx, _rx) = broadcast::channel::<TransferEvent>(16);
    *GLOBAL_SENDER.lock().unwrap() = Some(tx.clone());

    // WebSocket 路由：/ws
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .map(move |ws: warp::ws::Ws| {
            let tx = tx.clone();
            ws.on_upgrade(move |socket| client_connection(socket, tx))
        });

    // 启动 warp 服务
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move {
            warp::serve(ws_route)
                .run(([0, 0, 0, 0], port))
                .await;
        });
    });
}

/// 处理客户端连接
async fn client_connection(ws: WebSocket, tx: EventSender) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    // 订阅广播
    let mut rx = tx.subscribe();

    // 任务：接收客户端消息（可用于心跳）
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            if msg.is_close() { break; }
        }
    });

    // 任务：广播事件到客户端
    let send_task = tokio::spawn(async move {
        while let Ok(evt) = rx.recv().await {
            if let Ok(text) = serde_json::to_string(&evt) {
                let _ = ws_tx.send(Message::text(text)).await;
            }
        }
    });

    // 等待任一任务结束
    tokio::select! {
        _ = recv_task => (),
        _ = send_task => (),
    }
}

/// 上报事件
pub fn report_event(ev: TransferEvent) {
    if let Some(tx) = GLOBAL_SENDER.lock().unwrap().as_ref() {
        let _ = tx.send(ev);
    }
}

/// 文件发送示例
pub fn send_file(path: &str) {
    report_event(TransferEvent::Started { file: path.into() });
    // ... 执行传输逻辑 ...
    let ok = true;
    if ok {
        report_event(TransferEvent::Finished { file: path.into() });
    } else {
        report_event(TransferEvent::Failed { file: path.into(), err: "error".into() });
    }
}

/// 文件接收示例
pub fn receive_file(output_dir: &str) {
    println!("Listening for incoming files into directory: {}", output_dir);

    // 假设这里是接收逻辑，持续监听端口并处理文件
    loop {
        // 这里应换成实际接收逻辑
        std::thread::sleep(std::time::Duration::from_secs(1));

        // 示例：收到一个文件就触发事件（伪代码）
        let file_path = format!("{}/received_{}.txt", output_dir, chrono::Utc::now().timestamp());
        report_event(TransferEvent::Received { file: file_path.clone() });
        println!("Received and saved file: {}", file_path);
    }
}

// transfer-cli/src/main.rs
use clap::Parser;
use serde::Serialize;
use lazy_static::lazy_static;

#[derive(Parser)]
struct Args {
    #[clap(subcommand)]
    cmd: Command,
    /// 启用 WebSocket API 服务
    #[clap(long)]
    ws: bool,
    /// 服务监听端口
    #[clap(long, default_value = "3030")]
    port: u16,
}

#[derive(clap::Subcommand)]
enum Command {
    Send { file: String },
    Recv {
        #[clap(long, default_value = ".")]
        output_dir: String,
    },
}

fn main() {
    let args = Args::parse();

    if args.ws {
        init_ws_server(args.port);
        println!("WebSocket API 服务已启动，地址 ws://127.0.0.1:{}/ws", args.port);
    }

    match args.cmd {
        Command::Send { file } => send_file(&file),
        Command::Recv { output_dir } => receive_file(&output_dir),
    }
}
