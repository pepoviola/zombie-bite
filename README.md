# Zombie-bite

<div align="center">
<p>A cli tool to easily fork live networks (e.g kusama/polkadot).</p>
</div>

## :warning: :construction: Under Active Development :construction: :warning:

`zombie-bite` is a simple _cli_ tool that allow you to spawn a new network based on a live one (e.g kusama/polkadot). Under the hood we orchestrate the `sync` prior to _dump_ all the _state_ (except the consensus related keys) to a new chain-spec that you can use for spawning a new network with the live state.

## Usage

### Requerimients

In order to use this tool you will need this binaries available in your `PATH`

 - polkadot (and workers)
 - [chain-spec-generator](https://github.com/polkadot-fellows/runtimes/tree/main/chain-spec-generator) (from fellowship repo)
 - polkadot-parachain (to spawn with system parachains)


#### Fork `kusama` / `polkadot`

You can easily for `kusama` / `polkadot` running the following command:
```bash
cargo run -- kusama
```

* _NOTE_: pass `polkadot` as argument to fork it.


This will first sync a _temporarly_ node (using `warp` strategy) and then export all the state and create a new _chain-spec_ to spawn a new network. By default this network will contains four validators (Alice, Bob, Charlie and Dave).

_NOTE on `Governance`_: Since we are dumping all the state to a new chain-spec, the `governance` tab can/will display show a big offset since we are starting from block `0`.


### Fork with `system parachains`

You can include `system parachains` by passing them as argument in the command:


```bash
cargo run -- kusama asset-hub
```

:warning: This feature is working on progress and at the moment the parachain is spawned but _not produce blocks_.


---

### Steps for doppelganger :
  
Compile the node using [this branch](https://github.com/paritytech/polkadot-sdk/tree/jv-doppelganger-node) from the polkadot-sdk repo

```
cargo build -p polkadot-doppelganger-node --release
```

Make polkadot binaries (polkadot and workers) and doppelganger available in your PATH, then run with:

  ``` 
  RUST_LOG=zombienet=debug cargo run --bin doppelganger
  ```
  
  This will:

- Run doppelganger to sync (warp) kusama to a temp dir with the defaults overrides (2 nodes network)
- Generate the chain-spec without bootnodes
- Create a new snapshot to use with the new network in zombienet
- Spawn the new network and keep it running


