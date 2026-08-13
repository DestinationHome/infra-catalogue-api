<div align="center">

  <h1>🦀 <code>catalogue</code> 📦</h1>

  <p>
    <strong>High-performance GraphQL Catalogue API for the Destination Home project.</strong>
  </p>

  <p>
    <a href="https://github.com/DestinationHome/infra-catalogue-api/actions/workflows/lint.yml"><img src="https://img.shields.io/github/actions/workflow/status/DestinationHome/infra-catalogue-api/lint.yml?branch=main&style=flat-square&label=lint%20%26%20tests" alt="Build Status"></a>
    <a href="https://github.com/DestinationHome/infra-catalogue-api/pkgs/container/infra-catalogue-api"><img src="https://img.shields.io/badge/docker-ghcr.io-blue?style=flat-square&logo=docker" alt="Docker Image"></a>
    <a href="#license"><img src="https://img.shields.io/badge/license-AGPLv3-blue?style=flat-square" alt="License"></a>
  </p>

</div>

The **Catalogue API** (`catalogue`) is the high-performance GraphQL catalogue backend powering [Destination Home](https://destinationhome.live/catalogue). It indexes and serves over 87,000+ preservation items (spaces, clothing, furniture, apartments, rewards, and clubhouse assets) with instant typo tolerance, multilingual queries, and Relay-compliant cursor pagination.

---

## 🌟 Authors

- [@zeph](https://github.com/ZephyrCodesStuff) (that's me!)

## 🌠 Features

- 🏎️ **Sub-5ms Typo-Tolerant Search**: Built on [Meilisearch](https://www.meilisearch.com/) with native typo tolerance (`hodie` $\rightarrow$ `Hoodie`, `sceme` $\rightarrow$ `Scene`, `scrne` $\rightarrow$ `Scene`).
- 🌍 **Multilingual Localized Indexing**: Search across all 8+ PlayStation Home localization languages simultaneously (**English, Italian, Spanish, French, German, Japanese, Korean, and Chinese**) with automatic diacritic and accent normalization (`città` $\leftrightarrow$ `citta`).
- 🔒 **Zero-Trust Network Isolation**: Pre-configured Docker Compose architecture using internal networks. Neither Meilisearch nor MongoDB expose any open ports to the host or internet.
- 🪶 **Zero-OS Scratch Container**: Statically compiled against `musl` and packaged in a pure `FROM scratch` runtime container with zero OS attack surface and minimal memory footprint.

---

## 🏗️ Architecture

```
[ Web Browser / Client ]
           |
      (Port 8080)
           |
           v
   +---------------+
   | catalogue-api |  <--- Actix Web + Async-GraphQL (Rust)
   +---------------+
      /          \
     /            \  (internal_net: internal: true, 0 open host ports)
    v              v
+---------+  +-------------+
| MongoDB |  | Meilisearch |
+---------+  +-------------+
```

---

## 🧰 Getting Started

### Option A: Full Stack with Docker Compose (Recommended)

To start the complete stack (API, MongoDB, and isolated Meilisearch):

```bash
# Launch background services
docker compose up -d
```

This starts:
- **`catalogue-api`**: Listening on port `8080`
- **`mongodb`**: Secure internal network (port `27017` not exposed to host)
- **`meilisearch`**: Secure internal network (port `7700` not exposed to host)

---

### Option B: Local Cargo Development

If running MongoDB and Meilisearch locally:

1. Copy `.env.example` to `.env`:
   ```bash
   cp .env.example .env
   ```
2. Edit `.env` with your database and search URLs
3. Run the server:
   ```bash
   cargo run --release --bin catalogue
   ```

---

## ⚙️ Environment Variables

| Variable | Description | Default / Example |
| :--- | :--- | :--- |
| `HOST` | Server bind host address | `0.0.0.0` |
| `PORT` | Server HTTP port | `8080` |
| `MONGO_URI` | MongoDB connection URI with default database | `mongodb://localhost:27017/destination_home` |
| `JWT_PRIVATE_KEY` | HMAC SHA-512 private key for JWT authentication | `your_secret_key` |
| `MEILI_URL` | *(Optional)* Meilisearch server endpoint URL | `http://localhost:7700` |
| `RUST_LOG` | Logging verbosity filter | `info,catalogue=debug` |

---

## 🧪 Testing & Linting

```bash
# Run strict Clippy linter
cargo clippy --bins --release -- -D warnings -D clippy::nursery

# Run automated unit test suite
cargo test --bin catalogue
```

---

## 💛 Acknowledgements

- The **[Meilisearch](https://www.meilisearch.com)** team for building the fastest instant search engine.
- The **[Rust](https://www.rust-lang.org/)**, **[Actix](https://actix.rs/)**, and **[Async-GraphQL](https://async-graphql.github.io/)** communities.
- **Claude** and **Gemini** for autonomous pair-programming and refactoring support.

---

## 📝 License

This project is licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.

**What this means:**

- ✅ **You can** run, modify, and build upon this catalogue service.
- ✅ **You can** use this service for open-source preservation projects.
- 🛑 **If you host or modify this network service**, you **must** provide the complete corresponding source code to users under the AGPL-3.0.

See [LICENSE](LICENSE) for more details.
