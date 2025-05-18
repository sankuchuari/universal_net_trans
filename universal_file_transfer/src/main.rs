mod service;

use clap::{Parser, Subcommand};
use std::io::{self, ErrorKind};
use std::net::{IpAddr, ToSocketAddrs};
use url::Url;

fn resolve_url_to_ips(url_str: &str) -> io::Result<Vec<IpAddr>> {
    let url = Url::parse(url_str)
        .map_err(|e| io::Error::new(ErrorKind::InvalidInput, e))?;

    let hostname = url.host_str()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "URL 中未找到主机名"))?;
    let port = url.port_or_known_default()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "无法确定端口号"))?;
    // 解析域名
    let addrs = (hostname, port).to_socket_addrs()?;
    let ips: Vec<IpAddr> = addrs.map(|addr| addr.ip()).collect();
    Ok(ips)
}

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Send {
        server: String,
        port: u16,
        file: String,
        #[arg(long)]
        ws: bool,
    },
    Recv {
        output_dir: String,
        port: u16,
        #[arg(long)]
        ws: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 如果是 Send 且启用 ws，则解析 URL 第一个 IP
    let server_addr = match &cli.cmd {
        Commands::Send { server, ws: true, .. } => {
            resolve_url_to_ips(server)?.into_iter().next()
        }
        _ => None,
    };

    match cli.cmd {
        Commands::Recv { output_dir, port, ws } => {
            if ws {
                ws_recv(&output_dir, port).await?;
            } else {
                tcp_recv(&output_dir, port).await?;
            }
        }
        Commands::Send { server, port, file, ws } => {
            let target = server_addr.map_or(server.clone(), |ip| ip.to_string());
            if ws {
                ws_send(&target, port, &file).await?;
            } else {
                tcp_send(&target, port, &file).await?;
            }
        }
    }
    Ok(())
}

/// 接收端：监听 TCP，保存到文件
async fn tcp_recv(output_dir: &str, port: u16) -> anyhow::Result<()> {
    service::tcp_recv(output_dir, port).await
}

/// 发送端：连接 TCP，读取文件并发送
async fn tcp_send(server: &str, port: u16, file_path: &str) -> anyhow::Result<()> {
    service::tcp_send(server, port, file_path).await
}

/// 接收端：监听 WebSocket，保存到文件
async fn ws_recv(output_dir: &str, port: u16) -> anyhow::Result<()> {
    service::ws_recv(output_dir, port).await
}

/// 发送端：通过 WebSocket 发送文件
async fn ws_send(server: &str, port: u16, file_path: &str) -> anyhow::Result<()> {
    service::ws_send(server, port, file_path).await
}
