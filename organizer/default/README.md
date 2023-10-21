# Simple projects-and-folders organizer plugin for Clapshot.

This is a gRPC server, and serves a **default/example implementation**
of the Clapshot Organizer API.

This is written in Rust for maximum type safety (and consistency
with the main server), but you can write your own Organizer in any
language that supports gRPC - e.g. Python, Go, Java etc.

The actually interesting stuff is in lib.rs, which is the gRPC
server implementation. The rest is just boilerplate.
