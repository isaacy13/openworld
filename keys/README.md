# Release signing

No release signing key is pinned.

A key generated in a checkout would not be a durable release key, because the private half would not live anywhere you could use for the next release. Pin `keys/openworld-release.pub` when the release key exists. Until then, there is nothing here to trust, and there is no release digest to compare.

A checksum file shipped beside a binary is not the proof. The digest has to be the one on the GitHub release, and the signature has to match the pinned public key.
