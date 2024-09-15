# Node graph based IVOCT data processing application in Rust

## Try out this application

If you want to get familiar with this application, see this [application tutorial](doc/tutorial.md).

## Technical details

See the [Technical Overview](/doc/technical_overview.md).

## How to run

### 1. Install Rust

Using [rustup](https://www.rust-lang.org/tools/install).

Make sure Rusts bin folder is on your `PATH` by running `cargo --version`. You
may need to restart your terminal to have the changes take effect.

### 2. Clone repository

```
> git clone https://collaborating.tuhh.de/cem9903/ems_sose24_ivoct_testing.git -b gui-test-rust
> cd ems_sose24_ivoct_testing
```

#### 2.1 Open VS Code (Optional)

```
…\ems_sose24_ivoct_testing> code .
```

##### Install Rust extension

- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

### 3. Compile and run

```
…\ems_sose24_ivoct_testing> cargo run --release
```

> Note: Compiling the first time may take some time.
