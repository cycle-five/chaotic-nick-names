# The `firearms` category

`firearms` is a built-in **NSFW** name category (418 entries). Like the other
18+ categories it stays in the catalog and works on an explicit
`/randomize category:firearms`, shows under 🔞 in `/categories`, and is excluded
from the default random pool (`data::NSFW` in `src/data.rs`).

This document records *how* the list was curated, so the reasoning survives — it
was originally embedded in a one-shot `build_firearms.py` builder script, now
removed in favour of the reusable workflow in
[`.claude/skills/add-name-category/`](../.claude/skills/add-name-category/SKILL.md).

## Curation decisions

Three choices shaped the list:

1. **Entry scope — models *and* historic types.** Both specific named models
   (`Colt Python`, `AK-47`, `M1 Garand`) and broad historical classes
   (`Arquebus`, `Matchlock`, `Flintlock`, `Blunderbuss`). The class entries are
   what let the catalog reach "all the way back" to the 1400s rather than
   starting at the modern era.
2. **Naming — recognizable form.** Keep a manufacturer/qualifier prefix only when
   the bare designation would be cryptic (`Colt Python`, `Beretta 92`, `AK-47`),
   but go bare where the name is already iconic (`Luger`, `Tommy Gun`, `Garand`,
   `Arquebus`). The same physical gun is never listed under two aliases (e.g.
   `Tommy Gun` *or* `Thompson`, not both) — duplicates pass the uniqueness test
   but waste random-pool slots.
3. **Size — broad and historically spanning (~400–600).** A truly *complete*
   catalog of every firearm ever made runs to tens of thousands and can't be
   hand-curated accurately, so "complete" here means representative breadth, not
   exhaustiveness. 418 entries is comparable to the `cars` (262) / `spices` (417)
   categories.

## Era / type coverage

The list sweeps the whole history of small arms, roughly:

- **Ignition-era types & oddities** — hand cannon, arquebus, matchlock,
  wheellock, snaphance, miquelet, flintlock, caplock, blunderbuss, plus historic
  curios (Puckle gun, Nock gun, punt gun, duck-foot pistol, Gyrojet, Welrod).
- **Named muskets & early rifles** — Brown Bess, Charleville, Kentucky/Jaeger
  rifles, Whitworth, Dreyse needle gun, Chassepot.
- **Breechloaders & single-shot cartridge rifles** — Sharps, Spencer, Henry,
  Martini-Henry, rolling block.
- **Lever / pump repeaters** — the Winchester 1866→1907 line, Marlins, Savage 99.
- **Bolt-action military & sporting** — Mauser (Gewehr 98 / Kar98k), Mosin-Nagant,
  Lee-Enfield, Springfield 1903, Arisaka, Carcano, plus modern sporters.
- **Machine guns** — Gatling, Maxim, Vickers, Lewis, M2 Browning, MG 34/42,
  Minigun, and historic precursors (mitrailleuse, Nordenfelt, coffee-mill gun).
- **Submachine guns** — MP 18/40, Tommy gun, Sten, PPSh-41, Uzi, HK MP5, MAC-10.
- **Semi-auto / battle rifles & DMRs** — Garand, M14, FN FAL, HK G3, SKS, Dragunov.
- **Assault rifles** — StG 44, the AK family, M16/M4/AR-15, FAMAS, AUG, SCAR, etc.
- **Shotguns** — Winchester 1897, Remington 870, Mossberg 500, SPAS-12, plus
  types (coach gun, double barrel, sawed-off, lupara).
- **Semi-auto pistols** — Borchardt C93, Mauser C96, Luger, Colt 1911, Glock,
  SIG, CZ, HK, Desert Eagle.
- **Revolvers** — the Colt line (Paterson → Peacemaker → Python), S&W, Webley,
  Nagant, Ruger, LeMat.
- **Precision / anti-materiel** — Barrett M82, AWP, TAC-50, and historic
  anti-tank rifles (PTRD-41, Boys).

## Conventions enforced

Every name satisfies the invariants the `src/data.rs` tests check — ≤ 32 chars,
no surrounding whitespace, non-empty, unique within the category — and follows
the dataset's punctuation habits: **no `&`** (spell out or abbreviate, e.g.
`Smith Wesson`, `HK MP5`), **no apostrophes**, **no periods** (`Vz 58`, not
`Vz. 58`); hyphens and spaces are fine.

To add to or rebuild this (or any) category, use the
[`add-name-category`](../.claude/skills/add-name-category/SKILL.md) skill and its
`splice_category.py`, which enforce all of the above before writing.
