use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

mod cryptography;

/// 异步：TCP 模式下接收文件并保存
pub(crate) async fn tcp_recv(output_dir: &str, port: u16) -> anyhow::Result<()> {
    use tokio::net::TcpListener;
    use tokio::io::AsyncReadExt;
    use std::path::Path;
    use std::fs::File;
    use std::io::Write;
    use cryptography;
    use anyhow::Context;

    // 绑定监听 TCP 端口
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    println!("TCP 模式：监听端口 {}...", port);

    loop {
        // 接受新的连接
        let (mut socket, addr) = listener.accept().await?;
        println!("已与 {} 建立 TCP 连接", addr);

        let output_dir = output_dir.to_string();
        // 为每个连接创建独立任务
        tokio::spawn(async move {
            let client_addr = addr;
            let result: anyhow::Result<()> = async {
                // 1. 接收文件名长度及文件名
                let mut len_buf = [0u8; 4];
                socket.read_exact(&mut len_buf).await?;
                let filename_len = u32::from_be_bytes(len_buf) as usize;
                let mut filename_buf = vec![0u8; filename_len];
                socket.read_exact(&mut filename_buf).await?;
                let filename = String::from_utf8(filename_buf)
                    .unwrap_or_else(|_| "received.bin".to_string());

                // 2. 接收 SHA256 校验值（64 字节十六进制）
                let mut sha_buf = vec![0u8; 64];
                socket.read_exact(&mut sha_buf).await?;
                let sha_hex = String::from_utf8(sha_buf).unwrap_or_default();

                // 3. 接收加密参数：salt、nonce、ciphertext（均为十六进制），以及 16 字节密码文本
                let mut salt_hex_buf = [0u8; 32];
                socket.read_exact(&mut salt_hex_buf).await?;
                let salt_hex = String::from_utf8(salt_hex_buf.to_vec()).unwrap_or_default();

                let mut nonce_hex_buf = [0u8; 24];
                socket.read_exact(&mut nonce_hex_buf).await?;
                let nonce_hex = String::from_utf8(nonce_hex_buf.to_vec()).unwrap_or_default();

                let mut ct_hex_buf = [0u8; 64];
                socket.read_exact(&mut ct_hex_buf).await?;
                let ct_hex = String::from_utf8(ct_hex_buf.to_vec()).unwrap_or_default();

                let mut password_b_buf = [0u8; 16];
                socket.read_exact(&mut password_b_buf).await?;
                let password_b = String::from_utf8(password_b_buf.to_vec()).unwrap_or_default();

                // 4. 解密得到实际文件加密密码
                let pt = cryptography::decrypt_data(&password_b, &salt_hex, &nonce_hex, &ct_hex).unwrap();
                let password = String::from_utf8(pt).unwrap_or_default();

                // 5. 持续读取剩余的文件数据（加密后的内容）
                let mut data_buffer: Vec<u8> = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let n = socket.read(&mut buf).await?;
                    if n == 0 {
                        break;
                    }
                    data_buffer.extend_from_slice(&buf[..n]);
                }

                // 6. 生成唯一文件名以防覆盖
                let unique_name = generate_unique_filename(&output_dir, &filename, &client_addr);

                // 7. 将接收到的加密文件数据写入临时文件
                let enc_path = Path::new(&output_dir).join(format!("{}.enc", unique_name));
                let mut enc_file = File::create(&enc_path)
                    .with_context(|| format!("无法创建文件 {}", enc_path.display()))?;
                enc_file.write_all(&data_buffer)
                    .with_context(|| "写入加密数据失败")?;

                // 8. 解密文件内容到目标文件
                let decrypted_path = Path::new(&output_dir).join(&unique_name);
                cryptography::decrypt_file(
                    enc_path.to_str().unwrap(),
                    &password,
                    decrypted_path.to_str().unwrap(),
                ).with_context(|| "文件解密失败")?;

                // 9. 计算并校验 SHA256
                let sha_calculated = cryptography::calculate_sha256(decrypted_path.to_str().unwrap())
                    .with_context(|| "计算 SHA256 失败")?;
                if sha_calculated == sha_hex.to_lowercase() {
                    println!("文件已接收并保存为 {}", decrypted_path.display());
                } else {
                    println!("SHA256 校验失败: {} != {}", sha_hex, sha_calculated);
                }

                // 10. 清理临时加密文件
                std::fs::remove_file(enc_path).ok();

                Ok(())
            }.await;

            if let Err(e) = result {
                eprintln!("处理客户端 {} 时出错: {:?}", client_addr, e);
            }
        });
    }

    // （循环永不结束，不返回 Ok）
}

/// 异步：TCP 模式下发送文件
pub(crate) async fn tcp_send(server: &str, port: u16, file_path: &str) -> anyhow::Result<()> {
    let address = (server, port);
    let mut stream = TcpStream::connect(address).await?;
    println!("已通过 TCP 连接到 {}:{}", server, port);

    // 准备文件名和 SHA256
    let path = std::path::Path::new(file_path);
    let filename = path.file_name().unwrap().to_string_lossy().to_string();
    let filename_bytes = filename.as_bytes();
    let filename_len = filename_bytes.len() as u32;
    // 发送文件名长度和文件名
    stream.write_all(&filename_len.to_be_bytes()).await?;
    stream.write_all(filename_bytes).await?;
    // 发送 SHA256 校验值
    let sha256_str = cryptography::calculate_sha256(file_path)?;
    let mut sha_buf = vec![0u8; 64];
    sha_buf.copy_from_slice(sha256_str.as_bytes());
    stream.write_all(&sha_buf).await?;

    // 加密文件和加密密码
    let password = cryptography::password_creat();
    let password_b = cryptography::password_creat();
    let enc_path = "encrypt.enc";
    cryptography::encrypt_file(file_path, &*password, enc_path)?;
    let (salt_hex, nonce_hex, ct_hex) = cryptography::encrypt_data(&*password_b, password.as_ref());
    // 发送加密参数
    let mut salt_buf = [0u8; 32];
    let mut nonce_buf = [0u8; 24];
    let mut ct_buf = [0u8; 64];
    let mut pass_b_buf = [0u8; 16];
    salt_buf.copy_from_slice(salt_hex.as_bytes());
    nonce_buf.copy_from_slice(nonce_hex.as_bytes());
    ct_buf.copy_from_slice(ct_hex.as_bytes());
    pass_b_buf.copy_from_slice(password_b.as_bytes());
    stream.write_all(&salt_buf).await?;
    stream.write_all(&nonce_buf).await?;
    stream.write_all(&ct_buf).await?;
    stream.write_all(&pass_b_buf).await?;

    // 读取加密文件并发送
    let mut file = File::open(enc_path).await?;
    let mut buffer = [0u8; 4096];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 { break; }
        stream.write_all(&buffer[..n]).await?;
    }
    println!("文件 '{}' 发送成功。", file_path);
    fs::remove_file(enc_path).await?;
    Ok(())
}

/// 异步：WebSocket 模式下接收文件
pub(crate) async fn ws_recv(output_dir: &str, port: u16) -> anyhow::Result<()> {
    use tokio::net::TcpListener;
    use futures_util::StreamExt;
    use tokio_tungstenite::accept_async;
    use std::path::Path;
    use std::fs::File;
    use std::io::Write;
    use cryptography;
    use anyhow::Context;

    // 绑定监听 WebSocket 端口
    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    println!("WebSocket 模式：监听端口 {}...", port);

    loop {
        // 接受新的 TCP 连接
        let (stream, addr) = listener.accept().await?;
        println!("{} 已请求 WebSocket 连接", addr);

        let output_dir = output_dir.to_string();
        tokio::spawn(async move {
            let client_addr = addr;
            let result: anyhow::Result<()> = async {
                // 完成 WebSocket 握手
                let ws_stream = accept_async(stream).await
                    .context("WebSocket 握手失败")?;
                let (_write, mut read) = ws_stream.split();

                // 读取所有二进制消息并累积到缓冲区
                let mut data_buffer: Vec<u8> = Vec::new();
                while let Some(msg) = read.next().await {
                    let msg = msg?;
                    if msg.is_binary() {
                        data_buffer.extend_from_slice(&msg.into_data());
                    }
                }

                // 解析数据（与 TCP 方式相同：文件名、SHA、加密参数等）
                let mut offset = 0;
                // 文件名长度
                let filename_len = u32::from_be_bytes(
                    data_buffer[offset..offset+4].try_into().unwrap()
                ) as usize;
                offset += 4;
                // 文件名
                let filename = String::from_utf8(
                    data_buffer[offset..offset+filename_len].to_vec()
                ).unwrap_or_else(|_| "received.bin".to_string());
                offset += filename_len;
                // SHA256 校验值
                let sha_hex = String::from_utf8(
                    data_buffer[offset..offset+64].to_vec()
                ).unwrap_or_default();
                offset += 64;
                // salt (hex)
                let salt_hex = String::from_utf8(
                    data_buffer[offset..offset+32].to_vec()
                ).unwrap_or_default();
                offset += 32;
                // nonce (hex)
                let nonce_hex = String::from_utf8(
                    data_buffer[offset..offset+24].to_vec()
                ).unwrap_or_default();
                offset += 24;
                // ciphertext (hex)
                let ct_hex = String::from_utf8(
                    data_buffer[offset..offset+64].to_vec()
                ).unwrap_or_default();
                offset += 64;
                // 密码文本 (16 字节)
                let password_b = String::from_utf8(
                    data_buffer[offset..offset+16].to_vec()
                ).unwrap_or_default();
                offset += 16;

                // 解密得到实际文件加密密码
                let pt = cryptography::decrypt_data(&password_b, &salt_hex, &nonce_hex, &ct_hex).unwrap();
                let password = String::from_utf8(pt).unwrap_or_default();

                // 文件加密数据为余下部分
                let file_data = &data_buffer[offset..];

                // 生成唯一文件名
                let unique_name = generate_unique_filename(&output_dir, &filename, &client_addr);

                // 写入临时加密文件
                let enc_path = Path::new(&output_dir).join(format!("{}.enc", unique_name));
                let mut enc_file = File::create(&enc_path)
                    .with_context(|| format!("无法创建文件 {}", enc_path.display()))?;
                enc_file.write_all(file_data)
                    .with_context(|| "写入加密数据失败")?;

                // 解密到目标文件
                let decrypted_path = Path::new(&output_dir).join(&unique_name);
                cryptography::decrypt_file(
                    enc_path.to_str().unwrap(),
                    &password,
                    decrypted_path.to_str().unwrap(),
                ).with_context(|| "文件解密失败")?;

                // 校验 SHA256
                let sha_calculated = cryptography::calculate_sha256(decrypted_path.to_str().unwrap())
                    .with_context(|| "计算 SHA256 失败")?;
                if sha_calculated == sha_hex.to_lowercase() {
                    println!("WebSocket 文件已保存为 {}", decrypted_path.display());
                } else {
                    println!("SHA256 校验失败: {} != {}", sha_hex, sha_calculated);
                }

                // 清理临时加密文件
                std::fs::remove_file(enc_path).ok();

                Ok(())
            }.await;

            if let Err(e) = result {
                eprintln!("处理 WebSocket 客户端 {} 时出错: {:?}", client_addr, e);
            }
        });
    }

    // （循环永不结束，不返回 Ok）
}

/// 异步：WebSocket 模式下发送文件
pub(crate) async fn ws_send(server: &str, port: u16, file_path: &str) -> anyhow::Result<()> {
    let url = format!("ws://{}:{}/", server, port);
    let (ws_stream, _) = connect_async(url).await?;
    println!("已通过 WebSocket 连接到 {}:{}", server, port);

    let (mut write, _) = ws_stream.split();
    // 准备元数据 JSON
    let path = std::path::Path::new(file_path);
    let filename = path.file_name().unwrap().to_string_lossy().to_string();
    let sha256_str = cryptography::calculate_sha256(file_path)?;
    let password = cryptography::password_creat();
    let password_b = cryptography::password_creat();
    let enc_path = "encrypt.enc";
    cryptography::encrypt_file(file_path, &*password, enc_path)?;
    let (salt_hex, nonce_hex, ct_hex) = cryptography::encrypt_data(&*password_b, password.as_ref());

    let metadata = json!({
        "filename": filename,
        "sha256": sha256_str,
        "salt": salt_hex,
        "nonce": nonce_hex,
        "ct": ct_hex,
        "password_b": password_b
    });
    let metadata_str = serde_json::to_string(&metadata)?;
    write.send(Message::Text(metadata_str)).await?;

    // 发送加密后的文件内容（二进制帧）
    let mut file = File::open(enc_path).await?;
    let mut buffer = [0u8; 4096];
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 { break; }
        write.send(Message::Binary(buffer[..n].to_vec())).await?;
    }
    write.close().await?;
    println!("文件已通过 WebSocket 发送完成");
    fs::remove_file(enc_path).await?;
    Ok(())
}

/// 生成唯一文件名：若同名文件已存在，则添加客户端地址和时间戳后缀
fn generate_unique_filename(
    output_dir: &str,
    base_name: &str,
    addr: &std::net::SocketAddr
) -> String {
    let mut final_name = base_name.to_string();
    let path = std::path::Path::new(output_dir).join(&final_name);
    if path.exists() {
        // 获取当前时间戳（秒）
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        // 将地址中的 ':' 替换为 '_' 以避免文件名问题
        let addr_str = addr.to_string().replace(':', "_");
        // 分离文件名和扩展名
        if let Some(pos) = final_name.rfind('.') {
            let name_part = &final_name[..pos];
            let ext_part = &final_name[pos..];
            final_name = format!("{}_{}_{}{}", name_part, addr_str, ts, ext_part);
        } else {
            final_name = format!("{}_{}_{}", final_name, addr_str, ts);
        }
    }
    final_name
}
