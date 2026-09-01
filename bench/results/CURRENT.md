# Current: dealer3 source `28d0c086459a`, corpus `7d7033566ed1`

Measured 2026-09-01 on an Apple M4 Pro (12 logical, 8P+4E), best of 3 runs.
This is the "after" for removing the per-deal hand sort (`93cf0c2`).
The "before" is [BASELINE.md](BASELINE.md), source `92f902fd12e4`.

Reference numbers are unchanged and shared between the two: the three
reference programs did not move, and `corpus_id` is the same, so
`reference-7d7033566ed1.json` is valid for both.

```
dealer3 results : dealer3-28d0c086459a.json
                  source 28d0c086459a, repo v0.4.0-201-g93cf0c2
reference       : reference-7d7033566ed1.json
machine         : Apple M4 Pro  12 logical (8P+4E)

script                           exe*  dealer-c     V2_4   d3 -R1  d3 auto    vs C   vs V2
                                  M/s       M/s      M/s      M/s      M/s   (-R1)   (-R1)
----------------------------------------------------------------------------------------------
Scrambling_2NT                  0.231     2.754    1.319    3.190   10.211    1.2x    2.4x
GIB_1N_Basic                    0.226     2.806    1.307    2.995   10.332    1.1x    2.3x
Slam_After_Major_Fit            0.224     2.786    1.309    3.103    7.357    1.1x    2.4x
After_Partner_Overcalls         0.205     2.081    1.150    1.547    7.063    0.7x    1.3x
Major_Suit_Fit                  0.193     1.736    1.052    1.481    6.667    0.9x    1.4x
Drury                           0.209     2.226    1.184    1.144    5.190    0.5x    1.0x
Basic_Takeout_Double            0.169     1.193    0.874    0.762    4.000    0.6x    0.9x
GIB_1C-P-Resp                   0.175     1.299    0.910    0.668    3.497    0.5x    0.7x
Splinters_By_Opener             0.097     0.424    0.428    0.501    2.635    1.2x    1.2x
Gerber_By_Responder             0.098     0.331    0.386    0.378    2.241    1.1x    1.0x
----------------------------------------------------------------------------------------------
* dealer.exe runs x86-emulated on an ARM64 VM; dealer-c is the same
  source built natively, and is the fair comparison for its lineage.

dealer3 -R1 is 0.9x the original C dealer, built natively (geometric mean).
dealer3 -R1 is 7.0x dealer.exe as run on the VM (emulated -- not a like-for-like figure).
dealer3 -R1 is 1.3x DealerV2_4 across the corpus (geometric mean).
Threading (auto) buys 4.29x over single-threaded.

Where the time goes, per deal (ns)
               generate   evaluate      total   gen share
----------------------------------------------------------
dealer.exe         1485       4532       6017        25%
dealer-c            176        781        957        18%
DealerV2_4          269        943       1212        22%
dealer3 -R1         188        874       1062        18%
----------------------------------------------------------
Mean over the corpus. 'generate' is the _shuffle_baseline entry:
RNG and shuffle with a near-free condition.

Against the original C dealer: dealer3 is 1.1x SLOWER at generating and is 1.1x SLOWER at evaluating.

Against DealerV2_4: dealer3 is 1.4x faster at generating and is 1.1x faster at evaluating.

Against dealer.exe (emulated): dealer3 is 7.9x faster at generating and is 5.2x faster at evaluating.

Change against dealer3-92f902fd12e4.json
  source 92f902fd12e4 -> 28d0c086459a
script                           before      after     change
--------------------------------------------------------------
Scrambling_2NT                   1.519     3.190     2.10x faster
GIB_1N_Basic                     1.468     2.995     2.04x faster
Slam_After_Major_Fit             1.476     3.103     2.10x faster
After_Partner_Overcalls          1.004     1.547     1.54x faster
Major_Suit_Fit                   0.970     1.481     1.53x faster
Drury                            0.820     1.144     1.40x faster
Basic_Takeout_Double             0.596     0.762     1.28x faster
GIB_1C-P-Resp                    0.537     0.668     1.24x faster
Splinters_By_Opener              0.425     0.501     1.18x faster
Gerber_By_Responder              0.330     0.378     1.14x faster
_shuffle_baseline                1.872     5.324     2.84x faster
--------------------------------------------------------------
M deals/s, single-threaded (-R 1).

Overall: 1.60x faster across 11 scripts (geometric mean).

Thread scaling (corpus mean)
threads   speedup  efficiency                          
--------------------------------------------------------
      1     1.00x       100%  ####
      2     1.57x        78%  ######
      3     2.10x        70%  ########
      4     2.81x        70%  ###########
      5     3.18x        64%  ############
      6     3.54x        59%  ##############
      7     3.89x        56%  ###############
      8     3.85x        48%  ###############
     10     4.21x        42%  ################
     12     4.30x        36%  #################
--------------------------------------------------------
Peak 4.30x at 12 threads.
```
