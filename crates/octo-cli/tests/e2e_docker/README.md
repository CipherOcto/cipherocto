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

**The suite builds the image once per process, not once per scenario.**
This is a time saving, not a disk one, and the difference is measured:
BuildKit's cache is content-addressed and global, so the fourteen
redundant per-scenario builds were already hitting. Running the old
build-per-scenario pattern fifteen times costs 3.526 GB of build cache —
exactly what a single build costs. The redundancy was real; the disk
cost was not.

What the build runs under does not matter for the tag, because the tag is
explicit: compose writes `octo-e2e-l4:local` no matter which project built
it, so a scenario's own `up` finds the image already present and does not
rebuild. Isolation is unaffected, because the project name is what
separates the networks, volumes, and container names, and that is still
unique per scenario.

### Disk cost, measured

A full run of this suite costs about **3.5 GB of BuildKit cache**, and
BuildKit never garbage-collects. The cost therefore accumulates **per
distinct source state the suite is run against**, not per scenario and not
per build. During the round that measured this, a commit chain that
re-ran the suite against each commit left roughly 87 GB behind and filled
a 916 GB disk.

So: re-running this suite against unchanged source is close to free, and
re-running it against every commit of a long chain is what fills the disk.

The suite deliberately does **not** run `docker builder prune`. That would
be a shared-resource side effect on the developer's machine, discarding
the cache for every docker build on the host. If you reclaim the disk by
hand, that is the tradeoff you are making.

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
