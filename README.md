# Design Philosophy
1. 采用异步设计，尽量不使用 blocking 的 API, 或封装为 future.
2. 精简库依赖，非必要不添加依赖库.
3. 深思熟虑的API设计，保持代码精简与高度复用的同时降低复杂度与使用难度.