# rmr

## 1. 简介

用于学习 Rust Tokio 编程的项目。

测试方法如下：

先运行服务端：

```sh
RUST_LOG=info cargo run --bin server
```

然后运行客户端：

```sh
RUST_LOG=info cargo run --bin client
```

## 2. 编码原则

* 尽量遵守 Rust 编码的 Idiomatic
* 利用 log 配合 tracing 来输出日志
  * 设置日志级别：export RUST_LOG=info
* 利用 snafu 来创建 Error 类型，并遵循 snafu 提倡的 Error Handling philosophy
* 目前使用的 IDE 是：Neovim
