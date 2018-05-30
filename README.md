# local_cache_proxy


This is a project with a major goal and the more minor ones, unfortunately all the functional aspects are minor!


Major goal is to just learn/get better with rust.


minor goals:

Build a local bazel proxy, this can be run on a laptop or remote server.

Features to include:

[ ] Support binding to a unix socket or port.
    -- Bazel doesn't yet support connecting to a unix socket but there is a PR in flight, good for local security
[ ] Connect directly or via a unix socket based proxy. (http proxy support would be good too for completeness, maybe even socks proxy.)
[ ] Local LRU cache of files managed by the proxy to ensure it doesn't grow unbounded.
[ ] Ideally dynamically calculate our throughput to the upstream, and use that to drive decisions about actions to take
    Initally take options on the command line to specify thresholds
[ ] Use HTTP headers to determine file size of upstream content, return 404 to bazel if too large
[ ] Accept all uploads from local bazel, but only forward if file size is below threshold/reasonable to upload
[ ] Must ensure file is not on remote before upload (since we will have injected extra 404's)


Ideal:

Dynamically respond to both the target in question (TODO: how much can we extract from bazel about the cost involved in building a target?), and the users current internet connection (presumed asymetrical) to decide when to download vs build.


Some sort of random theory:

Setup a local shared cache around the local network (mDNS or similar things), this would be purely to provide higher throughput access to CAS blobs, which can have their hash checked by the client using the data from the AC which shouldn't use this cache and can have a list of known/trusted hosts.
