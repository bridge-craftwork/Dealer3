# Baseline: dealer3 source `92f902fd12e4`, corpus `7d7033566ed1`

Measured 2026-09-01 on an Apple M4 Pro (12 logical, 8P+4E), best of 3 runs.
This is the "before" for the deal-generation work: dealer3 as it stands,
with `sort_all_hands()` still running on every generated deal.

```
dealer3 results : dealer3-92f902fd12e4.json
                  source 92f902fd12e4, repo v0.4.0-197-g93bc1ec-dirty
reference       : reference-7d7033566ed1.json
machine         : Apple M4 Pro  12 logical (8P+4E)

script                           exe*  dealer-c     V2_4   d3 -R1  d3 auto    vs C   vs V2
                                  M/s       M/s      M/s      M/s      M/s   (-R1)   (-R1)
----------------------------------------------------------------------------------------------
Scrambling_2NT                  0.231     2.754    1.319    1.519    7.592    0.6x    1.2x
GIB_1N_Basic                    0.226     2.806    1.307    1.468    7.865    0.5x    1.1x
Slam_After_Major_Fit            0.224     2.786    1.309    1.476    8.036    0.5x    1.1x
After_Partner_Overcalls         0.205     2.081    1.150    1.004    5.121    0.5x    0.9x
Major_Suit_Fit                  0.193     1.736    1.052    0.970    5.202    0.6x    0.9x
Drury                           0.209     2.226    1.184    0.820    4.373    0.4x    0.7x
Basic_Takeout_Double            0.169     1.193    0.874    0.596    3.235    0.5x    0.7x
GIB_1C-P-Resp                   0.175     1.299    0.910    0.537    3.175    0.4x    0.6x
Splinters_By_Opener             0.097     0.424    0.428    0.425    2.219    1.0x    1.0x
Gerber_By_Responder             0.098     0.331    0.386    0.330    2.038    1.0x    0.9x
----------------------------------------------------------------------------------------------
* dealer.exe runs x86-emulated on an ARM64 VM; dealer-c is the same
  source built natively, and is the fair comparison for its lineage.

dealer3 -R1 is 0.6x the original C dealer, built natively (geometric mean).
dealer3 -R1 is 4.6x dealer.exe as run on the VM (emulated -- not a like-for-like figure).
dealer3 -R1 is 0.9x DealerV2_4 across the corpus (geometric mean).
Threading (auto) buys 5.42x over single-threaded.

Where the time goes, per deal (ns)
               generate   evaluate      total   gen share
----------------------------------------------------------
dealer.exe         1485       4532       6017        25%
dealer-c            176        781        957        18%
DealerV2_4          269        943       1212        22%
dealer3 -R1         534        884       1418        38%
----------------------------------------------------------
Mean over the corpus. 'generate' is the _shuffle_baseline entry:
RNG and shuffle with a near-free condition.

Against the original C dealer: dealer3 is 3.0x SLOWER at generating and is 1.1x SLOWER at evaluating.

Against DealerV2_4: dealer3 is 2.0x SLOWER at generating and is 1.1x faster at evaluating.

Against dealer.exe (emulated): dealer3 is 2.8x faster at generating and is 5.1x faster at evaluating.

Thread scaling (corpus mean)
threads   speedup  efficiency                          
--------------------------------------------------------
      1     1.00x       100%  ####
      2     1.70x        85%  ######
      3     2.19x        73%  ########
      4     3.08x        77%  ############
      5     3.54x        71%  ##############
      6     4.06x        68%  ################
      7     4.53x        65%  ##################
      8     4.79x        60%  ###################
     10     5.13x        51%  ####################
     12     5.30x        44%  #####################
--------------------------------------------------------
Peak 5.30x at 12 threads.
```
