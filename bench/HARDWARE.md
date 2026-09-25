# Reference hardware

Benchmarks and ADR numbers are measured on this machine unless stated otherwise.

| | |
|---|---|
| CPU | AMD Ryzen 5 3600, 6 cores / 12 threads |
| RAM | 32 GB |
| OS | Windows 11 Pro 10.0.26200, power plan "AMD Ryzen Balanced" |
| WebView | WebView2 / Edge 153 |
| Toolchain | Rust 1.98.1 (MSVC), MSVC 14.44 (VS 2022 Build Tools), LLVM 23.1.2 |
| MuPDF | 1.27.2 via `mupdf-sys` 0.8.0 |

Measurements taken with other desktop applications open (browser, music, ~39 % background CPU) are marked
as noisy in the corresponding ADR.
