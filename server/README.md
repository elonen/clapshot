# Clapshot server
## server component for Clapshot video review tool

[![Platforms](https://img.shields.io/badge/platforms-Win%20%7C%20OSX%20%7C%20Linux-blue)]()
[![Release](https://img.shields.io/github/v/release/elonen/clapshot?include_prereleases)]()

## TODO


## Installing

```
python3.9 -m venv venv
source venv/bin/activate
pip install --editable git://github.com/elonen/clapshot.git#egg=clapshot&subdirectory=server
```

...or if you wish to develop:

```
git clone git+ssh://git@github.com/elonen/clapshot.git
cd clapshot/server
./init-env.sh    # on Windows requires Mingw (Git Bash) or Cygwin
```

Either way, you can now type `lanscatter_master`, `lanscatter_peer` or `lanscatter_gui` on the command line.

## How it works

Master splits sync folder contents into _chunks_, calculates checksums, transfers them to different peers, and organizes peers to download chunks from each other optimally.

Changes on master's sync folder are mirrored to all connected peers, and any changes on peer sync folders are overwritten. Peers can leave and join the swarm at any time. Stopped syncs are automatically resumed.

According to simulations (see _Testing_ below) this should yield 50% – 90% distribution speed compared to ideal (unrealistic) simultaneous multicast – depending on other load on the nodes and network.

## Features

Features and notable differences to Btsync/Resilio, Syncthing and Dropbox-like software:

* It's a _one way synchronizer_ for distributing large folders 1-to-N, not a generic two-way syncer.
* Keeps only a _single copy of each file_ to save space – no `.sync` dirs with duplicate files.
* Centralized coordination, distributed transfers.  Few TCP connections, no broadcasts. (Peers connect to a master via websocket, and it instructs them to make HTTP downloads from each other or the master).
* Designed for _big chunk sizes_ to minimize coordination overhead. (Configurable, e.g. for deduplication if data is highly redundant.)
* Designed for _few simultaneous transfers_. This avoids unnecessary coordination traffic and overhead, assuming a reliable and fast LAN environment.
* Keeps traffic inside the LAN (doesn't connect to any third party servers).
* Resilient against slow individual nodes. Transfers from slow peers are detected, aborted and avoided afterwards.
* Does _not_ preserve Unix file attributes (for now), as Windows doesn't support them.
* Master never modifies sync directory - it treats it as _read only_.
* Supports bandwidth limiting.

## Technologies

Lanscatter is built on **Python 3.7** using **asyncio** (aiohttp & aiofiles),
**wxPython** for cross-platform GUI, multi-CPU chained **Blake2b** algorithm for chunk hashing, **LZ4** for in-flight compression,
**pytest** for unit / integration tests and **pyinstaller** for packaging / freezing into exe files.

It runs on Python 3.8 as well, but wxPython and pyinstaller seem to have some compatiblity issues currently
(_Jan 2020_), so you may run into trouble with GUI and freezing on 3.8+.

## Site-to-site distribution

LANScatter peers and masters can be chained into a distribution tree.

Master doesn't care where the files in sync directory come from, and never modifies them, so you can run a peer node
that downloads files directly into a master node's source directory. This kind of chained / proxy setup can be a useful if, for example,
you want to distribute files to another site over a VPN:

![Proxy setup](doc/chaining.svg)

Pointing peer nodes on LAN 2 directly to master on LAN1 is a bad idea as peers on different LANs will then start doing
P2P transfers over the slow VPN. The single swarm1 peer on LAN 2 doesn't have this problem, as swarm1 master will
notice it's a generally slow uploader, and will then avoid using it for p2p transfers on swarm1. You could also limit
its upload slots to 0.

LANScatter CLI and GUI tools don't have any special options for this setup yet, but it's easy to setup manually.
Master and peer already listen to different ports by default (10564 and 10565, respectively).

(In the future, a convenience command, perhaps called `lanscatter_proxy`, could streamline this setup.)  

## Building

Being a Python package, Lanscatter doesn't require building, but if you want to package .exe binaries for
Windows , run `./pyinstaller-build.sh` in _Git Bash_ (Mingw) or Cygwin.


## Architecture

Notable modules:

* **planner.py**: Protocol-agnostic distribution planner. Takes a list of chunk hashes and nodes, keeps track who has which chunks and outputs download suggestions. When ran as a stand-alone CLI program, it runs a **swarm simulation**, that can be used for testing and tuning the distribution strategy.

* **common.py**: Default constants and some common utils.

* **chunker.py**: Functions to scan a directory, split files into chunks and calculating checksums. Outputs `SyncBatch` class instances, that represent contents of scanned sync directory. Includes functions for comparing and (de)serializing them.

* **fileio.py**: Functions for uploading and downloading chunks from/to files on disk. Supports bandwidth throttling and limits operations inside a base directory (sync dir) for safety.

* **fileserver.py**: HTTP(S) server base that serves out chunks of files from sync dir. Used by both master node and peer nodes.

* **masternode.py**: CLI program that runs a master node.

* **peernode.py**: CLI program that runs a peer node.

* **gui.py**: Systray-based GUI for launching, controlling and monitoring both master nodes and peer nodes.

## Testing

The `tests` folder contains integration and unit tests using the _pytest_ framework; simply the environment and run `pytest`.

Integration test – in short – runs a master node and several peer node simultaneously, with random sync dir contents, and makes sure they get in sync without errors.

Planner is (unit)tested in isolation to make sure it terminates in a fully synced-up state. It simulates joins, drop-outs and slow peers, with an output that looks like this:

```
N00 ######################################################################## 0 4   0.3
N01 ####..#.####.##.....#................................................... 2 2   0.2
N02 #.###...##.#.#.......................................................... 2 2   0.3
N03 #.###...##.#.#.......................................................... 2 2   0.3
N04 #.####...#.##.....#..................................................... 2 0   25.1
N05 #.###.#....#..##..#...#.#..#............................................ 2 2   0.2
N06 ###.#..##......###..............#....................................... 2 2   28.5
N07 #####.#.#....###.#............#......................................... 2 2   0.3
N08 ##..#.#..###.###.........#....#......................................... 2 2   0.3
N09 ..##.....##...#.....#.#....###.#........................................ 2 2   0.3
N10 #.##..#.####........#....#...#.......................................... 2 2   0.3
N11 ##....#..###.###.##..................#.................................. 2 2   0.3
N12 .#.#..#...##.###.#....#........#........................................ 2 2   0.2
N13 ##.##.#.#....####....................................................... 2 2   0.3
N14 ##.#..#..##..##...#...#....#...#........................................ 2 2   0.3
N15 ..#.#...##..#....#.....##.#...#..#...................................... 2 2   0.3
N16 ##.#....#.##.###.#.....#.......#........................................ 2 2   0.3
N17 ...#..#.##...#..#.#.........#...#..##................................... 2 2   0.3
N18 .#..#.#.##.#...##.#.........#...#....................................... 2 0   23.7
N19 .##.#.#.#....#...#.....#................................................ 2 2   0.2
N20 ####..#..####....##..................................................... 2 2   0.3
N21 .#.##...#.#.######...............#...................................... 2 2   0.2
N22 ##..#.#...##...#.##....##............................................... 2 2   0.2
N24 .####....##.#.#####..................................................... 2 2   0.3
N25 .#......##.#.####.#...............#..................................... 2 2   0.3
N27 ..#.#....#..######.....#..#............................................. 2 2   0.3
N28 ...##...#.##.###...#....#............................................... 2 2   29.2
N29 ..##..#.###..##........#.#.............................................. 2 2   0.3
N30 #.###.#...##.##......................................................... 2 2   0.3
N31 #.#.#.#..###.##.......#................................................. 2 2   0.3
N32 ##.#..#.#.#..##.#....................................................... 2 2   28.1
N33 ##..#.#.#.##.###........................................................ 2 2   0.3
N34 ..##..#.###..#.........##............................................... 2 2   0.3
N35 .##.#.#.#..#...####..................................................... 2 2   0.2
N36 .........#...###.#..#....#..##.......................................... 2 2   0.3
N37 ...............#.##....##............................................... 2 2   0.3
Node join: N35
Node join: N36
Node join: N37
Slow download. Giving up. (from N04)
Slow download. Giving up. (from N04)
```

Left column is a list of node names.
Table with hash characters and dots show which chunks each node has.
Numbers on the right show current downloads, current uploads and average time it takes to upload one chunk from each node.

See `planner.plan_transfers()` for details on how planning algorithm works. Command
`python lanscatter/planner.py` runs the swarm simulation.

## License

Copyright 2019 Jarno Elonen <elonen@iki.fi>

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.