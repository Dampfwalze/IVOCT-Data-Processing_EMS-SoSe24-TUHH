# Node graph based IVOCT data processing application in Rust

Project for module __"Introduction into Medical Technology and Systems"__ at __TUHH__ in the summer semester of 2024.

Analyse IVOCT scans, find their lumen and generate a 3D mesh.

## Try out this application

If you want to get familiar with this application, see this [application tutorial](doc/tutorial.md).

## Technical details

See the [Technical Overview](/doc/technical_overview.md).

## Download

You can download an application build
[here](https://github.com/Dampfwalze/IVOCT-Data-Processing_EMS-SoSe24-TUHH/releases/tag/v1.0.0).

## How to run

### 1. Install Rust

Using [rustup](https://www.rust-lang.org/tools/install).

Make sure Rusts bin folder is on your `PATH` by running `cargo --version`. You
may need to restart your terminal to have the changes take effect.

### 2. Clone repository

```
> git clone https://github.com/Dampfwalze/IVOCT-Data-Processing_EMS-SoSe24-TUHH.git
> cd IVOCT-Data-Processing_EMS-SoSe24-TUHH
```

#### 2.1 Open VS Code (Optional)

```
…\IVOCT-Data-Processing_EMS-SoSe24-TUHH> code .
```

##### Install Rust extension

- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

### 3. Compile and run

```
…\IVOCT-Data-Processing_EMS-SoSe24-TUHH> cargo run --release
```

> Note: Compiling the first time may take some time.
