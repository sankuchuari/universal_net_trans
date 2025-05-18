use std::fs::File;
use std::io;
use std::io::{ErrorKind, Read, Write};
use aead::Key;
use aes_gcm::{Aes256Gcm, Nonce};
use hex::{decode, encode};
use hmac::Hmac;
use pbkdf2::pbkdf2;
use rand::Rng;
use rand_core::RngCore;
use sha2::{Digest, Sha256};
use aes_gcm::{aead::{Aead, KeyInit}};



//生成密钥
pub(crate) fn password_creat() -> String {
    use rand::{distributions::Alphanumeric, Rng};
    let random_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    random_string
}
//使用 PBKDF2 对文本进行加密（salt）和散列（hash）运算 (Encrypt (salt) and hash text using PBKDF2.)
const PBKDF2_ITERATIONS: u32 = 100_000; //设置 PBKDF2 的迭代次数 (Set the number of iterations for PBKDF2.)
const SALT_LENGTH: usize = 16; // 盐的长度 (length of salt)
const NONCE_LENGTH: usize = 12;
// 生成随机盐 （Generating Random Salt）
fn generate_salt() -> Vec<u8> {
    let mut rng = rand::thread_rng();
    let mut salt = vec![0u8; SALT_LENGTH];
    rng.fill(&mut salt[..]);
    salt
}
// 使用 PBKDF2 派生出 32 字节密钥 (Use PBKDF2 to derive 32-byte keys)
fn pbkdf2_hash(password: &str, salt: &[u8]) -> Vec<u8> {
    let mut output = vec![0u8; 32];
    // 这里 unwrap 是因为 pbkdf2 返回 ()
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut output).unwrap();
    output
}
// 加密函数
pub(crate) fn encrypt_data(password: &str, plaintext: &[u8]) -> (String, String, String) {
    let salt = generate_salt(); // salt 生成函数 
    let key_bytes = pbkdf2_hash(password, &salt); // PBKDF2 派生函数

    // 显式指定 key 类型 (Specify key type explicitly)
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));

    let mut nonce_bytes = [0u8; NONCE_LENGTH];
    rand::thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher.encrypt(nonce, plaintext)
        .expect("encryption failure!");

    (
        encode(&salt),
        encode(&nonce_bytes),
        encode(&ciphertext),
    )
}
// 解密函数 (decrypt function)
pub(crate) fn decrypt_data(
    password: &str,
    salt_hex: &str,
    nonce_hex: &str,
    ct_hex: &str
) -> Result<Vec<u8>, aes_gcm::Error> {
    let salt        = decode(salt_hex).expect("invalid salt hex");
    let nonce_bytes = decode(nonce_hex).expect("invalid nonce hex");
    let ciphertext  = decode(ct_hex).expect("invalid ciphertext hex");

    let key_bytes = pbkdf2_hash(password, &salt);

    // 显式指定 key 类型 (Specify key type explicitly)
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key_bytes));
    let nonce  = Nonce::from_slice(&nonce_bytes);

    cipher.decrypt(nonce, ciphertext.as_ref())
}


// 使用 AES-256-GCM 加密文件内容 (Encrypting File Contents with AES-256-GCM)
pub(crate) fn encrypt_file(path: &str, password: &str, out_path: &str) -> io::Result<()> {
    let mut file = File::open(path)?;
    let mut plaintext = Vec::new();
    file.read_to_end(&mut plaintext)?;

    // 盐 + 密钥生成 (Salt + Key Generation)
    let salt = generate_salt();
    let mut key_bytes = [0u8; 32];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), &salt, PBKDF2_ITERATIONS, &mut key_bytes).unwrap();
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes); //  显式指定类型

    // Nonce 生成 (Nonce Generation)
    let mut nonce_bytes = [0u8; 12];
    rand_core::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes); //  12字节 nonce

    // 加密 (encrypted)
    let cipher = Aes256Gcm::new(key);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_ref()).expect("加密失败");

    // 输出到文件：salt + nonce + ciphertext  (Output to file: salt + nonce + ciphertext)
    let mut output = File::create(out_path)?;
    output.write_all(&salt)?;
    output.write_all(&nonce_bytes)?;
    output.write_all(&ciphertext)?;

    Ok(())
}
//解密文件内容 (Decrypting the contents of a file)
pub(crate) fn decrypt_file(enc_path: &str, password: &str, out_path: &str) -> io::Result<()> {
    let mut file = File::open(enc_path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    // 检查内容长度是否合法 (Check for legal content length)
    if contents.len() < SALT_LENGTH + NONCE_LENGTH {
        return Err(io::Error::new(ErrorKind::InvalidData, "加密文件格式错误"));
    }

    // 提取 salt (前 16 字节) (Extract salt (first 16 bytes) )
    let salt = &contents[..SALT_LENGTH];
    // 提取 nonce (接下来的 12 字节) (Extract nonce (next 12 bytes) )
    let nonce_bytes = &contents[SALT_LENGTH..SALT_LENGTH + NONCE_LENGTH];
    // 剩下的是密文 (The rest is ciphertext.)
    let ciphertext = &contents[SALT_LENGTH + NONCE_LENGTH..];

    // 派生密钥 (derived key)
    let mut key_bytes = [0u8; 32];
    pbkdf2::<Hmac<Sha256>>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut key_bytes).unwrap();
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);

    // 初始化 AES-GCM (Initialise AES-GCM)
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(nonce_bytes);

    // 解密密文 (decrypted dense text)
    let decrypted_data = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "解密失败：密码错误或数据损坏"))?;

    // 写入输出文件 (Write to output file)
    let mut output_file = File::create(out_path)?;
    output_file.write_all(&decrypted_data)?;
    
    Ok(())
}


//计算并返回SHA265 (Calculates and returns SHA265)
pub(crate) fn calculate_sha256(file_path: &str) -> io::Result<String> {
    // 打开文件 (Open file)
    let mut file = File::open(file_path)?;

    // 创建 SHA-256 哈希计算器 (Creating a SHA-256 Hash Calculator)
    let mut hasher = Sha256::new();

    // 读取文件并更新哈希值 (Read the file and update the hash)
    let mut buffer = [0u8; 8192]; // 读取缓冲区 (readout buffer)
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    // 获取 SHA-256 哈希值并以十六进制字符串格式返回  (Gets the SHA-256 hash value and returns it as a hexadecimal string)
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}
