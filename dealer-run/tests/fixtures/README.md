# Input fixtures

`rpdd_10First.zrd` — the first ten records of Richard Pavlicek's solved-deal
library, as shipped with DealerV2_4. 230 bytes, ten 23-byte records, every one
of them solved and none of them a separator.

Copied from `bridge-encodings`' own fixture rather than read from that checkout:
a test that reaches outside this repository passes or fails on what is beside
it, which is not a property of this code.

It is the only input that can tell the format sniff from a lucky guess. Bytes
written here by a test would be bytes this code both wrote and read, and the
whole question is whether it reads a file it did not write.
