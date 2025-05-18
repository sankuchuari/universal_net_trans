# 📦 universal\_net\_trans

一个跨平台的安全文件传输工具，支持 TCP 与 WebSocket 协议，内置 AES 加密与 SHA256 校验。

## ✨ 项目简介

`universal_net_trans` 是由 Rust 构建的通用文件传输工具，支持通过 TCP 或 WebSocket 协议进行可靠、安全的点对点文件传输。支持命令行调用与图形界面集成，适合快速、安全的数据共享。

* 支持双向传输（发送 / 接收）
* 支持加密（AES）与完整性校验（SHA256）
* 支持 TCP 与 WebSocket 协议切换
* 提供 C++ 控制端文件选择器（Windows）

## 🖥️ 支持平台

* ✅ Windows（主支持平台）
* ⚙️ Linux（支持命令行构建与使用）

---

## ⚙️ 编译要求与依赖

### Rust CLI 端（universal\_file\_transfer）

* Rust 1.70+（推荐使用 rustup）
* OpenSSL（用于 cryptography 模块）

### Windows 构建依赖

* MSVC toolchain
* OpenSSL v1.1+（建议使用 [slproweb 安装包](https://slproweb.com/products/Win32OpenSSL.html)）

### Linux 构建依赖

* `libssl-dev`
* `build-essential`

### C++ 控制端（仅限 Windows）

* 编译器支持 C++17（如 MSVC 或 g++）
* 链接库：`shell32`, `comdlg32`, `ole32`
* 支持 WinAPI GUI 函数调用（用于文件对话框）

---

## 🐧 Linux 下编译教程

```bash
# 安装 Rust
curl https://sh.rustup.rs -sSf | sh

# 安装 OpenSSL 开发包
sudo apt update
sudo apt install -y pkg-config libssl-dev build-essential

# 构建
cd universal_net_trans
cargo build --release

# 可执行文件生成位置
./target/release/universal_file_transfer
```

---

## 🧩 命令行使用说明
仅提供windows环境下示例，其他环境请使用编译得到的可执行文件。
```bash
# 接收文件（TCP 模式）
universal_file_transfer.exe recv <保存目录> <端口>

# 接收文件（WebSocket 模式）
universal_file_transfer.exe recv <保存目录> <端口> --ws

# 发送文件（TCP 模式）
universal_file_transfer.exe send <服务器地址> <端口> <文件路径>

# 发送文件（WebSocket 模式）
universal_file_transfer.exe send <服务器地址> <端口> <文件路径> --ws
```

📌 示例：

```bash
universal_file_transfer.exe recv "./downloads" 9000
universal_file_transfer.exe send 127.0.0.1 9000 "./file.txt" --ws
```

---

## 🖱️ C++ 控制端说明（仅 Windows）

该程序提供简单交互菜单，通过 WinAPI 弹出图形化文件选择器，然后拼接命令行调用 Rust 编译生成的 `universal_file_transfer.exe`。

### 编译要求

* Windows 系统
* g++ 或 MSVC，需链接：`comdlg32`, `shell32`, `ole32`

### 编译示例（MinGW）

```bash
g++ controller.cpp -o controller.exe -lcomdlg32 -lshell32 -lole32
```

### 功能示例

* 启动接收端 → 选择保存目录 → 指定端口
* 启动发送端 → 选择文件 → 输入目标地址 + 端口

> 控制端自动通过 system() 执行 Rust CLI 程序，无需用户手动输入路径。



---

## 📌 C++ 控制端完整代码示例

```cpp
#include <iostream>
#include <string>
#include <cstdlib>
#include <direct.h>
#include <limits.h>
using namespace std;

#ifdef _WIN32
#define CLEAR "cls"
#else
#define CLEAR "clear"
#endif

const string EXECUTABLE_PATH = ".\\runtime_environment\\Universal_File_Transfer.exe";
const string CURRENT_DIR = ".\\saved_files";
const int PATH_MAX = 1024;

// 获取当前目录
string get_current_dir() {
    char buffer[PATH_MAX];
    if (_getcwd(buffer, sizeof(buffer)) != NULL) {
        return string(buffer);
    } else {
        return string();
    }
}

// 启动接收端
void start_receiver(const string& output_dir, int port, bool ws) {
    string cmd = "cd /d " + get_current_dir();
    system(cmd.c_str());
    cmd = EXECUTABLE_PATH + " recv \"" + output_dir + "\" " + to_string(port);
    if (ws) {
        cmd += " --ws";
    }
    cout << "[启动接收端] 命令: " << cmd << endl;
    system(cmd.c_str());
}

// 启动发送端
void send_file(const string& server, int port, const string& file_path, bool ws) {
    string cmd = "cd /d " + get_current_dir();
    system(cmd.c_str());
    cmd = EXECUTABLE_PATH + " send " + server + " " + to_string(port) + " \"" + file_path + "\"";
    if (ws) {
        cmd += " --ws";
    }
    cout << "[启动发送端] 命令: " << cmd << endl;
    system(cmd.c_str());
}

int main() {
    int choice;
    cout << "=== 通用文件传输 控制程序 ===\n";
    cout << "1. 启动接收端\n";
    cout << "2. 启动发送端\n";
    cout << "请输入选项: ";
    cin >> choice;

    if (choice == 1) {
        string output_dir = CURRENT_DIR;
        int port;
        char use_ws;
        char use_current_dir;

        cout << "是否使用默认保存目录.\\saved_files (y/n)";
        cin >> use_current_dir;
        if (use_current_dir == 'n' || use_current_dir == 'N') {
            cout << "请输入输出目录: ";
            cin >> output_dir;
        }
        cout << "请输入监听端口: ";
        cin >> port;
        cout << "使用 WebSocket 模式? (y/n): ";
        cin >> use_ws;

        start_receiver(output_dir, port, use_ws == 'y' || use_ws == 'Y');
    }
    else if (choice == 2) {
        string server, file_path;
        int port;
        char use_ws;

        cout << "请输入目标服务器地址 (IP 或 ws://...): ";
        cin >> server;
        cout << "请输入端口: ";
        cin >> port;
        cout << "请输入要发送的文件路径: ";
        cin >> file_path;
        cout << "使用 WebSocket 模式? (y/n): ";
        cin >> use_ws;

        send_file(server, port, file_path, use_ws == 'y' || use_ws == 'Y');
    } else {
        cout << "无效选项。\n";
    }

    return 0;
}
```
---

## 👤 作者与许可证

* 作者：Sankuchuari
* 协议：GNU General Public License v3.0

> 本项目完全开源，欢迎使用、修改与分发。
