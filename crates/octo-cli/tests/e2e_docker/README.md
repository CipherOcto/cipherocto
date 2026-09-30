# Layer 4 fixtures — cross-container multi-node `octo`

Fixtures for `crates/octo-cli/tests/e2e_l4_multinode_docker.rs`, the
cross-container half of the multi-peer end-to-end programme. Read the test
file's module header first; it states the isolation model and the two facts
about `octo` that shape every scenario here.

| File                 | Role                                                            |
| -------------------- | --------------------------------------------------------------- |
| `Dockerfile`         | builds `octo` from the committed source on the pinned toolchain |
| `compose-2node.yaml` | two nodes, two volumes, one bridge                              |
| `compose-3node.yaml` | three nodes, for the full-mesh scenario                         |
| `entrypoint.sh`      | prepares one node's `$OCTO_HOME`, then idles                    |

## Running

```bash
# Requires a running docker engine and compose v2.
cargo test -p octo-cli --test e2e_l4_multinode_docker -- --ignored --test-threads=1
```

Every scenario is `#[ignore]`d so `cargo test` on a machine without docker
stays green, and opt-in so the image build never lands in an unrelated test
run.

The first invocation builds the node image, which is a full release build of
`octo` and its dependency tree. It is cached by docker afterwards, but any
change under `crates/` or the escaped root crates invalidates the compile
layer.

## Building the image by hand

Only needed when debugging a build failure. The context is the workspace root
because `octo` is a workspace member with path dependencies:

```bash
docker build -f crates/octo-cli/tests/e2e_docker/Dockerfile -t octo-e2e-l4:local .
```

The `COPY` list is not the whole repository, and that is deliberate — the
context is 2 GB and most of it is irrelevant. Two path dependencies live
outside `crates/` and must be copied by name or cargo fails during
resolution:

- `octo-sync` — a leaf crate excluded from the member glob, pulled in by
  `octo-network` as `../../octo-sync`
- `determin` — the deterministic-floating-point crate, referenced as
  `../../determin`

## Two things this harness deliberately does not fake

**`octo` has no daemon.** There is no `serve` subcommand, so a node container
holds no long-running `octo` process. The entrypoint prepares the home and
idles; the suite drives `octo` with `docker compose exec`. A harness that
implied otherwise would be testing a process the binary does not have.

**`octo` opens no listening socket.** The reachability scenarios start the
listener they probe with `netcat`, deliberately outside `octo`. They prove
that an address _recorded in the peer table_ is routable from the recording
node's network namespace — which is the claim worth testing — without
asserting that `octo` serves anything.

## Teardown

`Compose` implements `Drop` and runs `docker compose down -v
--remove-orphans`, so a panicking scenario does not leak a stack, its volumes,
and its network. Each scenario also takes a unique project name, so scenarios
are safe to run concurrently and leftovers from one cannot be mistaken for
another. `l4_teardown_removes_every_volume_and_network_the_scenario_owned`
asserts the teardown actually works rather than assuming it.
