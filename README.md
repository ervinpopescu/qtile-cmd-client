# Qtile Command Client written in `Rust`

<!--toc:start-->

- [Qtile Command Client written in `Rust`](#qtile-command-client-written-in-rust)
  - [IMPORTANT](#important)
  - [TODO](#todo)
  <!--toc:end-->

This pet project was born out of an issue I could not find the root cause for:

```bash
time (qtile cmd-obj -f windows &>/dev/null)

real    4.73s
user    1.09s
sys     3.64s
cpu     99%
```

No benchmarking needed:

```bash
time (cargo run --release -q -- cmd-obj -f windows &>/dev/null)

real    0.06s
user    0.00s
sys     0.00s
cpu     2%
```

## IMPORTANT

THIS PROJECT IS STILL IN THE EARLY STAGES!

## TODO

- Add tests
