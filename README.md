# 🎧 VoxFlow AI

VoxFlow AI is a desktop application designed to streamline the creation of audiobooks and audio content. Built with modern web and Rust technologies, it allows you to visually script, synthesize natural-sounding speech using Alibaba Cloud Bailian, and manage your audio projects with ease.

> **Note:** This application uses the **Alibaba Cloud Bailian (DashScope)** platform for its Text-to-Speech (TTS) capabilities. You will need a valid API Key from the platform to use the synthesis features.

[![VoxFlow Demo](./docs/demo-screenshot.png)](https://github.com/iMyth/VoxFlow)

---

## ✨ Features

- 🎙️ **Alibaba Cloud Bailian Integration** — High-quality TTS synthesis powered by models like Qwen3 TTS Flash.
- ⚙️ **User-Configurable Settings** — Easily input your API Key, select models, and adjust default voice settings (speed, pitch, voice) directly from the application UI.
- ⏱️ **Adjustable Intervals** — Fine-tune the silence between script lines to ensure natural pacing.
- 💾 **Robust Data Management** — Built-in SQLite database ensures data integrity and prevents conflicts during manual script creation.
- 🖥️ **Modern Desktop Experience** — Powered by Tauri 2.0 for a lightweight, secure, and native feel.

---

## 🚀 Getting Started

Follow these instructions to get a local copy of the project up and running.

### Prerequisites

- [Node.js](https://nodejs.org/) (v18 or later)
- [Rust](https://www.rust-lang.org/) (latest stable version)

### Installation

1. **Clone the repository:**

   ```bash
   git clone https://github.com/iMyth/VoxFlow.git
   cd VoxFlow
   ```

2. **Install dependencies:**

   ```bash
   # Install frontend dependencies
   pnpm install
   # Or: npm install
   ```

3. **Configure Bailian API:**

   Unlike many CLI tools, VoxFlow does not require you to set environment variables manually.

   - Launch the application (see [Development](#development) below).
   - Click the **Settings** (gear icon ⚙️).
   - Enter your **DashScope API Key** in the designated field.

---

## 🧑‍💻 Development

To start the application in development mode with hot-reloading:

```bash
npm run tauri:dev
```

This command will start both the Vite development server and the Tauri backend simultaneously.

---

## 📦 Building for Production

To build the application for your platform:

```bash
npm run tauri:build
```

---

## 🛠️ Tech Stack

| Layer            | Technologies                                          |
|------------------|-------------------------------------------------------|
| **Frontend**     | React 19, TypeScript, Vite, Tailwind CSS, Zustand     |
| **Backend**      | Rust, Tauri 2.0, Tokio, Rusqlite (SQLite)             |
| **AI Services**  | Alibaba Cloud Bailian (DashScope)                     |

---

## 📄 License

This project is licensed under the [MIT License](./LICENSE).
