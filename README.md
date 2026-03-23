# FNDR

Here is a comprehensive `README.md` file for the FNDR project, incorporating the system architecture, features, and the vision for an agentic future.

***

# ⌘ FNDR
### The "Cmd+F" for your digital life. 

**Team:** Anurup Kumar, Felipe Marin Pena, Kunj Rathod, Minh Le  
**Course:** CS 4000 Capstone 2025-2026

---

## 📖 Overview
Modern knowledge work produces vast amounts of transient digital information—error messages, unsaved drafts, and context that traditional file searches cannot find. While recent industry attempts (like Microsoft Recall) have tried to solve this by continuously recording the screen, they created a massive "Privacy Paradox," turning the user's device into a security honeypot by storing raw screenshots.

**FNDR is a local-first, privacy-preserving semantic memory system for macOS**. It fundamentally reframes the problem: *you don't need to store what the user saw; you only need to store what it meant*. 

By using an **"Instant Distillation"** pipeline, FNDR captures screen activity, extracts text and semantic context immediately, converts it into vector embeddings, and **irreversibly destroys all raw visual data**.

---

## ✨ Key Features

*   **Total Semantic Recall:** Search your past activity by meaning, intent, or vague memory (e.g., "the document where I was angry about the delay") rather than exact keywords. 
*   **Hybrid Intelligence Search:** Combines Vector Search (for semantic relevance) and Full-Text Search (for exact keywords) using Reciprocal Rank Fusion (RRF) for high-precision results.
*   **Semantic Reconstruction Cards:** FNDR never stores or displays raw screenshots. Search results are stylized cards showing the app icon, window title, and an OCR text snippet with search terms highlighted.
*   **Temporal Memory & Timelines:** Ask time-based questions like "What was I working on last Tuesday?" or scrub through a virtualized daily timeline of reconstructed activity.
*   **Strict Privacy Controls:** Includes an app/website blocklist to prevent capturing sensitive apps (e.g., 1Password, banking), an "Incognito Mode" pause toggle, and clear data deletion options.
*   **The Agentic Future (Memory → Action):** FNDR goes beyond passive Q&A. It acts as a continuous, local context engine that extracts TODOs and passes them to local AI agents. In the future, FNDR will fuel parallel autonomous agents to orchestrate workflows across your apps without needing manual prompting.

---

## 🏗️ Architecture & How It Works

FNDR is engineered in **Rust** to enforce memory safety and maintain a sub-1% CPU resource footprint so it can run continuously in the background.

1.  **High-Performance Capture:** Uses macOS `ScreenCaptureKit` for zero-copy streaming directly from the GPU, sampling at an efficient 0.5–1 FPS.
2.  **pHash Deduplication:** Downsamples frames to 32x32 and uses Perceptual Hashing to calculate visual similarity. If the screen hasn't changed, the frame is instantly dropped, saving up to 90% of processing power.
3.  **Vision OCR:** Frames with significant changes are passed to Apple's Vision Framework, utilizing the hardware-accelerated Neural Engine (ANE) to extract text.
4.  **Instant Distillation (Zeroization):** The moment text is extracted, the raw pixel data and memory buffers are securely sanitized using Rust's `zeroize` crate, preventing ghost data from remaining in memory.
5.  **Local Embedding:** The text is processed through the `all-MiniLM-L6-v2` transformer model (via ONNX Runtime or Candle) to generate 384-dimensional semantic vectors.
6.  **Serverless Storage:** Vectors and metadata are stored in **LanceDB**, an embedded database allowing zero-copy reads and hybrid (Vector + FTS) search.

---

## 🛡️ Defense-In-Depth (Security)

FNDR assumes that even a database of numbers (vectors) could be a honeypot if someone attempts an "Embedding Inversion" attack. We implement two core defenses:
*   **Dimensionality Reduction:** Vectors are subjected to Principal Component Analysis (PCA) or Binary Quantization to reduce dimensions (e.g., from 384 to 128), destroying the fine-grained data needed to reconstruct readable text.
*   **Differential Privacy:** A noise vector drawn from a Laplacian distribution is added to the embeddings before storage, ensuring retrieval accuracy while making text reconstruction highly unreliable.

---

## 🛠️ Tech Stack

| Component | Library/Tool | Role |
| :--- | :--- | :--- |
| **Backend / Core** | Rust | Memory-safe systems programming & daemon logic |
| **Frontend UI** | Tauri v2 + React | Lightweight native webview replacing Electron |
| **Screen Capture** | `screencapturekit-rs` | macOS zero-copy screen recording |
| **OCR** | Apple Vision Framework | Native text extraction via `objc2` |
| **Machine Learning** | `candle` / ONNX | Local LLM and embedding inference on ANE/Metal |
| **Database** | LanceDB | Local, embedded vector storage & FTS |
| **UI Virtualization** | `react-window` | Efficient rendering of up to 10k timeline elements |

---

## 🚀 Getting Started 

*(Note: FNDR is under active development as a CS 4000 Capstone Project).*

1. **Initial Setup:** On first launch, the onboarding wizard will guide you through FNDR's local-first philosophy.
2. **Permissions:** You will be prompted to grant macOS `Screen Recording` and `Accessibility` permissions so FNDR can capture context.
3. **Model Download:** The system will automatically download the necessary local AI models (e.g., Llama, SmolVLM, MiniLM) directly to your machine. No API keys or cloud subscriptions are required.
4. **Run:** FNDR will securely run in the background. Use your global hotkey to open the FNDR search bar and start querying your memory.



## Getting started

To make it easy for you to get started with GitLab, here's a list of recommended next steps.

Already a pro? Just edit this README.md and make it your own. Want to make it easy? [Use the template at the bottom](#editing-this-readme)!

## Add your files

- [ ] [Create](https://docs.gitlab.com/ee/user/project/repository/web_editor.html#create-a-file) or [upload](https://docs.gitlab.com/ee/user/project/repository/web_editor.html#upload-a-file) files
- [ ] [Add files using the command line](https://docs.gitlab.com/topics/git/add_files/#add-files-to-a-git-repository) or push an existing Git repository with the following command:

```
cd existing_repo
git remote add origin https://capstone.cs.utah.edu/fndr/fndr.git
git branch -M main
git push -uf origin main
```

## Integrate with your tools

- [ ] [Set up project integrations](https://capstone.cs.utah.edu/fndr/fndr/-/settings/integrations)

## Collaborate with your team

- [ ] [Invite team members and collaborators](https://docs.gitlab.com/ee/user/project/members/)
- [ ] [Create a new merge request](https://docs.gitlab.com/ee/user/project/merge_requests/creating_merge_requests.html)
- [ ] [Automatically close issues from merge requests](https://docs.gitlab.com/ee/user/project/issues/managing_issues.html#closing-issues-automatically)
- [ ] [Enable merge request approvals](https://docs.gitlab.com/ee/user/project/merge_requests/approvals/)
- [ ] [Set auto-merge](https://docs.gitlab.com/user/project/merge_requests/auto_merge/)

## Test and Deploy

Use the built-in continuous integration in GitLab.

- [ ] [Get started with GitLab CI/CD](https://docs.gitlab.com/ee/ci/quick_start/)
- [ ] [Analyze your code for known vulnerabilities with Static Application Security Testing (SAST)](https://docs.gitlab.com/ee/user/application_security/sast/)
- [ ] [Deploy to Kubernetes, Amazon EC2, or Amazon ECS using Auto Deploy](https://docs.gitlab.com/ee/topics/autodevops/requirements.html)
- [ ] [Use pull-based deployments for improved Kubernetes management](https://docs.gitlab.com/ee/user/clusters/agent/)
- [ ] [Set up protected environments](https://docs.gitlab.com/ee/ci/environments/protected_environments.html)

***

# Editing this README

When you're ready to make this README your own, just edit this file and use the handy template below (or feel free to structure it however you want - this is just a starting point!). Thanks to [makeareadme.com](https://www.makeareadme.com/) for this template.

## Suggestions for a good README

Every project is different, so consider which of these sections apply to yours. The sections used in the template are suggestions for most open source projects. Also keep in mind that while a README can be too long and detailed, too long is better than too short. If you think your README is too long, consider utilizing another form of documentation rather than cutting out information.

## Name
Choose a self-explaining name for your project.

## Description
Let people know what your project can do specifically. Provide context and add a link to any reference visitors might be unfamiliar with. A list of Features or a Background subsection can also be added here. If there are alternatives to your project, this is a good place to list differentiating factors.

## Badges
On some READMEs, you may see small images that convey metadata, such as whether or not all the tests are passing for the project. You can use Shields to add some to your README. Many services also have instructions for adding a badge.

## Visuals
Depending on what you are making, it can be a good idea to include screenshots or even a video (you'll frequently see GIFs rather than actual videos). Tools like ttygif can help, but check out Asciinema for a more sophisticated method.

## Installation
Within a particular ecosystem, there may be a common way of installing things, such as using Yarn, NuGet, or Homebrew. However, consider the possibility that whoever is reading your README is a novice and would like more guidance. Listing specific steps helps remove ambiguity and gets people to using your project as quickly as possible. If it only runs in a specific context like a particular programming language version or operating system or has dependencies that have to be installed manually, also add a Requirements subsection.

## Usage
Use examples liberally, and show the expected output if you can. It's helpful to have inline the smallest example of usage that you can demonstrate, while providing links to more sophisticated examples if they are too long to reasonably include in the README.

## Support
Tell people where they can go to for help. It can be any combination of an issue tracker, a chat room, an email address, etc.

## Roadmap
If you have ideas for releases in the future, it is a good idea to list them in the README.

## Contributing
State if you are open to contributions and what your requirements are for accepting them.

For people who want to make changes to your project, it's helpful to have some documentation on how to get started. Perhaps there is a script that they should run or some environment variables that they need to set. Make these steps explicit. These instructions could also be useful to your future self.

You can also document commands to lint the code or run tests. These steps help to ensure high code quality and reduce the likelihood that the changes inadvertently break something. Having instructions for running tests is especially helpful if it requires external setup, such as starting a Selenium server for testing in a browser.

## Authors and acknowledgment
Show your appreciation to those who have contributed to the project.

## License
For open source projects, say how it is licensed.

## Project status
If you have run out of energy or time for your project, put a note at the top of the README saying that development has slowed down or stopped completely. Someone may choose to fork your project or volunteer to step in as a maintainer or owner, allowing your project to keep going. You can also make an explicit request for maintainers.
