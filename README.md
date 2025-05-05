# Design Philosophy
1. 采用异步设计，尽量不使用 blocking 的 API, 或封装为 future.
2. 精简库依赖，非必要不添加依赖库,除非被依赖依赖的库.
3. 深思熟虑的API设计，保持代码精简与高度复用的同时降低复杂度与使用难度.

### Q&A
1. Q：pull后启动报错? A: 尝试删除config.json

## 构建说明

使用
```powershell
cargo build --release
```
来构建 release 配置

或者通过
```powershell
cargo build --profile fat-release
```
来构建矿泉水写的 release 配置
