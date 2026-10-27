# Cold and dark start (81-717) — only the cab's own controls

Run: `viewer --cab --at 300` (needs `game/content/maps/crossline_c4/map.bsp`; `--noworld` to skip the map).
Mouse: click a button/switch (hold the button for hold-type keys); drag empty space to look. F2 orbit / F3 cab.
(`F5` runs this whole sequence by script; `F6` resets the ARS autostop.) Verified by the test `cold_and_dark_manual_start`.

1. **Battery**: VB (mouse) — all breakers start closed on this train. The overload relay (RP) trips at power-up.
2. **Brakes**: click the three valve cocks `DriverValveBLDisconnect`, `DriverValveTLDisconnect`, `DriverValveDisconnect`,
   brake handle to position 1 (keys `Num1`); wait ~8 s for the brake cylinders to empty (BC 2.7 -> 0).
3. **Doors**: VUD1 on; with one car there is no second cab to latch KD, so switch VAD (door-circuit bypass) on.
4. **Wrench and direction**: `Num Enter` (wrench to KV), `Num +` (reverser forward).
5. **Reset RP**: KV to 0 (`4`), hold the RP return button (`VozvratRP`) ~0.5 s.
6. **Traction relay**: KV to T1 (`5`) for 2+ s, then `4` (0) and **immediately** `3` (X3) — RVT only holds ~1 s.
7. Drive: `1` X1, `2` X2, `3` X3; `4` 0; `5` T1, `6` T1A, `7` T2; brake handle `Num1..7`/`F`,`R`.
8. ARS stand-in: speed above the ALS code of the next signal (+3 km/h) trips the autostop; `F6` releases it.

9. Doors: hold `A` (left) or `D` (right) to open, `V` (VUD1) to close; closing takes about 6 s.
