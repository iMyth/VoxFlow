# 🎧 VoxFlow AI: Audiobook Pipeline Visual Orchestrator

A full-stack desktop application for visualizing and orchestrating audiobook production workflows. Built with **Tauri 2.0** and **React 19**, VoxFlow allows you to script, synthesize with Alibaba Cloud, and mix audio seamlessly.

> **Note:** This project utilizes the **Alibaba Cloud Bailian (百炼)** TTS platform for voice synthesis.

[![VoxFlow Demo](./docs/demo-screenshot.png)](https://github.com/iMyth/VoxFlow)

## ✨ Features

- **🎙️ High-Quality TTS:** Powered exclusively by **Alibaba Cloud Bailian** for professional-grade voice synthesis.
- **🎛️ Visual Scripting:** Drag-and-drop interface to arrange your audiobook scripts.
- **⏱️ Adjustable Timing:** Fine-tune the flow of your narration with configurable silence intervals between lines.
- **💾 Robust Data:** Ensured data integrity prevents foreign key conflicts during manual script creation.
- **⚡ Native Performance:** Rust (Tauri) backend ensures a lightweight and fast desktop experience.

## 🛠️ Tech Stack

| Layer | Technology | Version | Purpose |
| :--- | :--- | :--- | :--- |
| **Frontend** | React | 19.1.0 | UI & User Experience |
| **Styling** | Tailwind CSS | 4.2.2 | Utility-First Styling |
| **Runtime** | Tauri | 2.x | Desktop Shell (Rust) |
| **Backend** | Rust | - | Business Logic & System APIs |
| **Database** | SQLite | rusqlite 0.32 | Local Data Persistence |
| **Audio** | rodio/cpal | 0.19/0.15 | Audio Playback & Mixing |
| **HTTP** | reqwest/tokio | 0.12 / 1.x | Async Networking |

## 🚀 Getting Started

### Prerequisites

- **Node.js** (>= 18.x) & **pnpm** (or npm)
- **Rust & Cargo** (Install via [rustup](https://www.rust-lang.org/tools/install))
- **Alibaba Cloud Account** (To obtain Bailian API Key)

### Installation

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/iMyth/VoxFlow.git
    cd VoxFlow
    ```

2.  **Install dependencies:**
    ```bash
    # Install frontend dependencies
    pnpm install
    # Or npm install
    ```

3.  **Configure Bailian API:**
    Create a `.env` file in the project root (frontend) or handle it in Tauri config as needed:
    ```env
    # Get your API Key from: https://bailian.console.aliyun.com/
    ALIBABA_CLOUD_API_KEY=your_actual_api_key_here
    ```

4.  **Run the App:**
    ```bash
    # Start the development server
    pnpm tauri dev
    # Or npm run tauri dev
    ```

## 📂 Project Structure

This project follows the standard Tauri 2.0 structure with a focus on separation of concerns:

```text
VoxFlow-AI/
├── src/                  # React 19 Frontend (TypeScript)
├── src-tauri/
│   ├── src/              # Rust Backend (Commands, State, DB)
│   │   ├── main.rs       # Entry point
│   │   └── lib.rs        # Core logic
│   ├── build.rs          # Tauri build script
│   ├── Cargo.toml        # Rust dependencies (Tauri, SQL, HTTP)
│   └── tauri.conf.json   # Tauri configuration
├── .env                  # Environment variables
├── pnpm-lock.yaml        # Dependency lockfile
└── README.md