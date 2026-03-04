# chaotic-nick-names

A Discord bot that assigns randomly-chosen nicknames from themed categories to
server members.  Names are drawn **without replacement** — every name in a
category is used before any name repeats.

Built with the [Serenity](https://github.com/serenity-rs/serenity) Discord
library for Rust (via the [Poise](https://github.com/serenity-rs/poise)
slash-command framework).

---

## Features

| Feature | Details |
|---------|---------|
| **Slash commands** | Registered globally; work in every server the bot joins |
| **Randomize everyone** | `/randomize` renames every non-bot member in the server |
| **Rename one user** | `/nick @user` renames a single member |
| **Context-menu command** | Right-click any user → *Assign Random Nick* |
| **Without-replacement draws** | The full pool is exhausted before any name repeats |
| **8 built-in categories** | scientists, elements, chemical\_compounds, amusement\_parks, dinosaurs, planets, colors, fruits |
| **Custom categories** | Server admins can add (and remove) their own name lists |
| **Statistics** | `/stats` shows total changes, bulk-randomize runs, and top-used categories |
| **History** | `/history` shows the last 25 nickname changes |
| **Pool reset** | `/reset_pool` lets a name re-enter the draw before the pool is exhausted |

---

## Slash commands

### `/randomize [category]`
Assigns a random nickname from `category` to every non-bot member.  
If `category` is omitted, a random category is chosen.  
**Requires:** Manage Nicknames permission.

### `/nick <user> [category]`
Assigns a single random nickname to `<user>`.  
If `category` is omitted, a random category is chosen.  
**Requires:** Manage Nicknames permission.

### `/categories list`
Lists all available categories (built-in and custom) with their name counts.

### `/categories add <name> <items>`
Adds a custom category.  `<items>` is a comma-separated list of nickname values.  
**Requires:** Manage Server permission.

### `/categories remove <name>`
Removes a custom category (built-in categories cannot be removed).  
**Requires:** Manage Server permission.

### `/stats`
Shows aggregate statistics for the current server:
- Total nickname changes
- Number of `/randomize` runs
- Top 5 most-used categories

### `/history [limit]`
Shows the most recent nickname changes (1–25 entries, default 10).

### `/reset_pool [category]`
Resets the without-replacement pool for `category` (or all categories), so
previously assigned names become available for selection again.  
**Requires:** Manage Nicknames permission.

---

## Context-menu command

Right-click (or long-press on mobile) any server member → *Apps* →
**Assign Random Nick** to assign a name from a randomly chosen category.  
**Requires:** Manage Nicknames permission.

---

## Built-in categories

| Category | Example names |
|----------|--------------|
| `scientists` | Einstein, Curie, Turing, Feynman… |
| `elements` | Hydrogen, Gold, Uranium… |
| `chemical_compounds` | Caffeine, Dopamine, Ethanol… |
| `amusement_parks` | Cedar Point, Dollywood, Europa Park… |
| `dinosaurs` | T-Rex, Velociraptor, Spinosaurus… |
| `planets` | Mercury, Saturn, Pluto… |
| `colors` | Crimson, Cerulean, Chartreuse… |
| `fruits` | Dragonfruit, Persimmon, Kumquat… |

---

## Setup

### Prerequisites
- Rust 1.70+ (`rustup` recommended)
- A Discord application with a bot token ([Discord Developer Portal](https://discord.com/developers/applications))

### Required bot permissions
The bot requires the following permissions when invited to a server:

| Permission | Why |
|------------|-----|
| **Manage Nicknames** | To change member nicknames |
| **Read Messages / View Channels** | To receive slash-command interactions |

Invite URL scope: `applications.commands` + `bot`.

### Privileged gateway intents
Enable **Server Members Intent** in the Discord Developer Portal
(*Bot → Privileged Gateway Intents*).  This is required so the bot can
list all members when `/randomize` is used.

### Configuration

```bash
cp .env.example .env
# Edit .env and set DISCORD_TOKEN=your_token_here
```

### Build & run

```bash
cargo build --release
./target/release/chaotic-nick-names
```

Or for development:

```bash
DISCORD_TOKEN=your_token cargo run
```

Commands are registered **globally** on first startup (Discord propagation can
take up to one hour).

---

## License

MIT
