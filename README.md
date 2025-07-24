# Zombie-bite

<div align="center">
<p>A cli tool to easily fork live networks (e.g kusama/polkadot).</p>
</div>

## :warning: :construction: Under Active Development :construction: :warning:

> Currently this tool is focused in the AH migration project and isn't ready to use as general purpouse tool without customization.


`zombie-bite` is a simple _cli_ tool that allow you to spawn a new network based on a live one (e.g kusama/polkadot). Under the hood we orchestrate the `sync` prior to _bite_ and spawn a new network with the _live state_.

## Methods
Currently there are two different _bite_ methods `doppelganger`(based on https://github.com/paritytech/polkadot-sdk/issues/4230) and `fork-off` that is inspired in the different _fork-off_ scripts (https://hack-ink.github.io/subalfred/user/cli/state.html / https://github.com/maxsam4/fork-off-substrate).

### Doppelganger usage


#### Requerimients

In order to use this tool you will need this binaries available in your `PATH`

 - [polkadot](https://github.com/paritytech/polkadot-sdk)
 - [polkadot-parachain](https://github.com/paritytech/polkadot-sdk) (in order to spawn system parachains)
 - [Doppelganger binaries](https://github.com/paritytech/doppelganger-wrapper) (doppelganger, doppelganger-parachain, workers)


#### Steps for doppelganger:

Make polkadot binaries (polkadot, polkadot-parachain) and (doppelganger, doppelganger-parachain, workers) available in your PATH, then you need to go back to this [_repo_](https://github.com/pepoviola/zombie-bite) and run this command to spawn polkadot and asset-hub from the live chains:

  ```
  RUST_LOG=zombienet=debug cargo run -- polkadot asset-hub
  ```

This will:

- Run doppelganger-parachain to sync (warp) asset-hub to a temp dir with the defaults overrides (2 nodes network)
- Run doppelganger to sync (warp) polkadot to a temp dir with the defaults overrides (2 nodes network)
- Generate the chain-spec _without bootnodes_
- Create a new snapshot to use with the new network in zombienet
- Spawn the new network and keep it running (_note_: you need to wait a couple of minutes to bootstrap)

##### Override runtime (wasm)

If you need to override the runtime of the releaychain or any parachain to be spawned, you need to use the _cli syntax_ <chain:path_to_wasm> and zombie-bite will read the wasm from the path and update the _state_ of the chain to use the new one in the spawned network.

e.g:

```sh
cargo run -- polkadot:./runtime_wasm/polkadot_runtime.compact.compressed.wasm asset-hub:./runtime_wasm/asset_hub_polkadot_runtime.compact.compressed.wasm
```

##### Log level:

By default the nodes are spawned with this log leves:

```sh
babe=trace,grandpa=trace,runtime=debug,consensus::common=trace,parachain=debug,sync=debug
```
_but_ you can override those by setting the `RUST_LOG` env, since the script will inject that env into the spawning logic.


##### Override / Inject Keys:

Zombie-bite create a json file including two maps (`overrides` and `injects`), these two are simple key/values json that zombie-bite pass to the _doppelganger nodes_ to override/inject those keys in the _block import_ process. Those _nodes_ `override` the key IFF the key exist in the _state being imported_ and `inject` the ones sets at the end of the import process, so will be present in the resulting state even if there wasen't there originally.

You can check the keys we override/inject by default (for both [relaychain](https://github.com/pepoviola/zombie-bite/blob/main/src/overrides.rs#L8) / [parachain](https://github.com/pepoviola/zombie-bite/blob/main/src/overrides.rs#L136)) and at the moment if you want to include other key (or customize one) yo need to modify this [file](https://github.com/pepoviola/zombie-bite/blob/main/src/overrides.rs) and rebuild the tool. _Note_: a process to dynamically set the overrides/injects map is planned.


---

> :warning::warning: This methods isn't fully workable yet :warning::warning:

#### Fork-off `kusama` / `polkadot`

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


