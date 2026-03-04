# chaotic-nick-names

A Discord bot that assigns randomly-chosen nicknames from themed categories to
server members.  Names are drawn **without replacement** — every name in a
category is used before any name repeats.

Built with the [Serenity](https://github.com/serenity-rs/serenity) Discord
library for Rust (via the [Poise](https://github.com/serenity-rs/poise)
slash-command framework).  State is persisted to **PostgreSQL** via
[sqlx](https://github.com/launchbadger/sqlx) so history, stats, custom
categories, and name-pool state survive bot restarts.

---

## Features

| Feature | Details |
|---------|---------|
| **Slash commands** | Registered globally; work in every server the bot joins |
| **Randomize everyone** | `/randomize` renames every non-bot member in the server |
| **Rename one user** | `/nick @user [category] [specific_name]` renames a single member |
| **Context-menu command** | Right-click any user → *Assign Random Nick* (modal lets you pick category and/or a specific name) |
| **Without-replacement draws** | The full pool is exhausted before any name repeats |
| **8 built-in categories** | scientists, elements, chemical\_compounds, amusement\_parks, dinosaurs, planets, colors, fruits |
| **Custom categories** | Server admins can add (and remove) their own name lists inline or via CSV file upload |
| **Statistics** | `/stats` shows total changes, bulk-randomize runs, and top-used categories |
| **History** | `/history` shows the last 25 nickname changes |
| **Pool reset** | `/reset_pool` lets a name re-enter the draw before the pool is exhausted |
| **PostgreSQL persistence** | All state is stored in Postgres; the bot reloads from the DB on startup |

---

## Slash commands

### `/randomize [category]`
Assigns a random nickname from `category` to every non-bot member.  
If `category` is omitted, a random category is chosen.  
**Requires:** Manage Nicknames permission.

### `/nick <user> [category] [specific_name]`
Assigns a nickname to `<user>`.  
- `category` — which category to draw from (random if omitted)  
- `specific_name` — assign an exact name from the chosen category (random from category if omitted)  
**Requires:** Manage Nicknames permission.

### `/categories list`
Lists all available categories (built-in and custom) with their name counts.

### `/categories add <name> <items>`
Adds a custom category.  `<items>` is a comma-separated list of nickname values.  
**Requires:** Manage Server permission.

### `/categories remove <name>`
Removes a custom category (built-in categories cannot be removed).  
**Requires:** Manage Server permission.

### `/categories import <file>`
Import one or more categories from a CSV file attachment.  
Each row: `category_name,name1,name2,…`  
Multiple rows create/replace multiple categories at once.  
Lines starting with `#` and blank lines are skipped.  
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
**Assign Random Nick** to assign a nickname.  
A modal lets you optionally specify:
- **Category** — leave blank for a random category
- **Specific name** — leave blank to pick randomly from the chosen category

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

## CSV import format

Upload a plain-text `.csv` (or `.txt`) file to `/categories import`.  
Each row becomes one category:

```
# Lines starting with # are ignored
scientists,Einstein,Newton,Darwin,Curie,Tesla
elements,Hydrogen,Helium,Lithium,Carbon,Oxygen
my_parks,Cedar Point,Dollywood,Tivoli Gardens
```

Existing categories with the same name are replaced and their used-name pools
are reset.

---

## Setup

### Prerequisites
- Rust 1.70+ (`rustup` recommended)
- PostgreSQL 14+ (any recent version works)
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
# Edit .env and fill in DISCORD_TOKEN and DATABASE_URL
```

The `.env` values:

| Variable | Description |
|----------|-------------|
| `DISCORD_TOKEN` | Your bot token from the Developer Portal |
| `DATABASE_URL` | Postgres connection string, e.g. `postgres://user:pw@localhost/chaotic_nick_names` |
| `RUST_LOG` | Log filter (optional, default `chaotic_nick_names=info,warn`) |

### Database setup

Create the database (the bot runs migrations automatically on startup):

```sql
CREATE DATABASE chaotic_nick_names;
```

### Build & run

```bash
cargo build --release
./target/release/chaotic-nick-names
```

Or for development:

```bash
DISCORD_TOKEN=your_token DATABASE_URL=postgres://... cargo run
```

Commands are registered **globally** on first startup (Discord propagation can
take up to one hour).

---

## License

MIT

