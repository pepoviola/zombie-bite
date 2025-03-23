# Zombie-bite

<div align="center">
<p>A cli tool to easily fork live networks (e.g kusama/polkadot).</p>
</div>

## :warning: :construction: Under Active Development :construction: :warning:

`zombie-bite` is a simple _cli_ tool that allow you to spawn a new network based on a live one (e.g kusama/polkadot). Under the hood we orchestrate the `sync` prior to _bite_ and spawn a new network with the _live state_.

## Usage

### Requerimients

In order to use this tool you will need this binaries available in your `PATH`

 - polkadot (and workers)
 - [chain-spec-generator](https://github.com/polkadot-fellows/runtimes/tree/main/chain-spec-generator) (from fellowship repo)
 - polkadot-parachain (to spawn with system parachains)

### Methods
Currently there are two different _bite_ methods `doppelganger`(based on https://github.com/paritytech/polkadot-sdk/issues/4230) and `fork-off` that is inpired in the different _fork-off_ scripts (https://hack-ink.github.io/subalfred/user/cli/state.html / https://github.com/maxsam4/fork-off-substrate).

#### Steps for doppelganger:

Compile the node/s using [this branch](https://github.com/paritytech/polkadot-sdk/tree/jv-doppelganger-node) from the polkadot-sdk repo

```
SKIP_WASM_BUILD=1 cargo build --release -p polkadot-doppelganger-node --bin doppelganger
SKIP_WASM_BUILD=1 cargo build --release -p polkadot-parachain-bin --features doppelganger --bin doppelganger-parachain
SKIP_WASM_BUILD=1 cargo build --release -p polkadot-parachain-bin --bin polkadot-parachain
SKIP_WASM_BUILD=1 cargo build --release --bin polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker
```

Make polkadot binaries (polkadot, polkadot-parachain and workers) and (doppelganger, doppelganger-parachain) available in your PATH, then you need to go back to this [_repo_](https://github.com/pepoviola/zombie-bite) and run this command to spawn polkadot and asset-hub from the live chains:

  ```
  RUST_LOG=zombienet=debug cargo run -- polkadot asset-hub
  ```

This will:

- Run doppelganger-parachain to sync (warp) asset-hub to a temp dir with the defaults overrides (2 nodes network)
- Run doppelganger to sync (warp) polkadot to a temp dir with the defaults overrides (2 nodes network)
- Generate the chain-spec without bootnodes
- Create a new snapshot to use with the new network in zombienet
- Spawn the new network and keep it running (_note_: you need to wait a couple of minutes to bootstrap)


_Log level for nodes_: By default the nodes are spawned with this log leves:
```
babe=trace,grandpa=trace,runtime=debug,consensus::common=trace,parachain=debug,sync=debug
```
_but_ you can override those by setting the `RUST_LOG` env, since the script will inject that env into the spawning logic.

---

#### Fork `kusama` / `polkadot`

You can easily for `kusama` / `polkadot` running the following command:
```bash
cargo run -- kusama fork-off
```

* _NOTE_: pass `polkadot` as argument to fork it.


This will first sync a _temporarly_ node (using `warp` strategy) and then export all the state and create a new _chain-spec_ to spawn a new network. By default this network will contains four validators (Alice, Bob, Charlie and Dave).

_NOTE on `Governance`_: Since we are dumping all the state to a new chain-spec, the `governance` tab can/will display show a big offset since we are starting from block `0`.


### Fork with `system parachains`

You can include `system parachains` by passing them as argument in the command:


```bash
cargo run -- kusama asset-hub fork-off
```

:warning: This feature is working on progress and at the moment the parachain is spawned but _not produce blocks_.


