# 🔄 Fork & Upstream Sync Strategy

This guide explains how to maintain your personal fork of Meetily while staying up-to-date with the original project.

## 🎯 Goal

Keep your custom configuration in a separate branch while being able to pull updates from the upstream repository without conflicts.

## 📋 Strategy Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    YOUR GITHUB FORK                              │
│                                                                  │
│  main ─────────────────────────────────────────────────────────  │
│    │                                                             │
│    │   (periodically sync from upstream)                        │
│    │                                                             │
│    └──► custom/remote-setup ─────────────────────────────────   │
│              │                                                   │
│              │  Your custom configuration lives here:           │
│              │  • .env files with your IPs                      │
│              │  • docker-compose tweaks                         │
│              │  • Any personal modifications                    │
│              │                                                   │
│              └──► (rebase on main after sync)                   │
└─────────────────────────────────────────────────────────────────┘
```

## 🚀 Initial Setup

### 1. Fork the Repository

1. Go to https://github.com/Zackriya-Solutions/meeting-minutes
2. Click "Fork" → Create fork on your account

### 2. Clone Your Fork

```bash
git clone https://github.com/YOUR_USERNAME/meeting-minutes.git
cd meeting-minutes

# Add upstream remote
git remote add upstream https://github.com/Zackriya-Solutions/meeting-minutes.git

# Verify remotes
git remote -v
# origin    https://github.com/YOUR_USERNAME/meeting-minutes.git (fetch)
# origin    https://github.com/YOUR_USERNAME/meeting-minutes.git (push)
# upstream  https://github.com/Zackriya-Solutions/meeting-minutes.git (fetch)
# upstream  https://github.com/Zackriya-Solutions/meeting-minutes.git (push)
```

### 3. Create Your Custom Branch

```bash
# Create branch for your customizations
git checkout -b custom/remote-setup

# Make your customizations (already done by this session)
# Then commit them
git add .
git commit -m "feat: custom remote setup for Tailscale + GPU server

- Configure MEETILY_BACKEND_URL and MEETILY_OLLAMA_URL
- Update docker-compose for GPU (RTX 4090)
- Remove frontend Docker (using native Tauri app)
- Add remote desktop setup documentation"

# Push to your fork
git push -u origin custom/remote-setup
```

## 🔄 Syncing with Upstream

When the original project gets updates you want:

### Method 1: Rebase (Recommended for Clean History)

```bash
# Fetch latest from upstream
git fetch upstream

# Update your main branch
git checkout main
git merge upstream/main
git push origin main

# Rebase your custom branch on updated main
git checkout custom/remote-setup
git rebase main

# If conflicts occur, resolve them, then:
git rebase --continue

# Force push the rebased branch
git push --force-with-lease origin custom/remote-setup
```

### Method 2: Merge (Simpler but Messier History)

```bash
# Fetch latest from upstream
git fetch upstream

# Update your main branch
git checkout main
git merge upstream/main
git push origin main

# Merge main into your custom branch
git checkout custom/remote-setup
git merge main

# Resolve any conflicts, then:
git push origin custom/remote-setup
```

## 📁 Files to Keep in Custom Branch

These files contain your personal configuration and should stay in your custom branch:

| File | Purpose |
|------|---------|
| `backend/.env` | Your Docker configuration (GPU, ports, language) |
| `frontend/.env` | Your Tailscale IPs |
| `.gitignore` additions | Any personal ignore patterns |

## 🔒 Files to NEVER Commit

Add these to your local `.git/info/exclude` (not tracked):

```
# Local-only files
backend/.env
frontend/.env
*.local
```

Alternatively, create `.env` from `.env.example` and keep `.env` in `.gitignore`.

## 🎛️ Best Practice: Use .env.example + .env

The project already follows this pattern:
1. `.env.example` files are tracked (templates with defaults)
2. `.env` files are gitignored (your actual configuration)

This means:
- Upstream updates to `.env.example` won't conflict with your `.env`
- Your personal configuration is never accidentally pushed

## 📊 Workflow Diagram

```
Upstream main                Your Fork
    │                             │
    │                         main
    │                             │
    ▼                             ▼
[New Feature]  ─────sync────►  main (updated)
                                  │
                                  │ rebase
                                  ▼
                         custom/remote-setup
                                  │
                                  │ Your customizations
                                  ▼
                           [Your working copy]
```

## 🛠️ Automation Script

Create `sync-upstream.sh` in your repo root:

```bash
#!/bin/bash
# Sync fork with upstream and rebase custom branch

set -e

echo "📥 Fetching upstream..."
git fetch upstream

echo "🔄 Updating main..."
git checkout main
git merge upstream/main --ff-only
git push origin main

echo "🔀 Rebasing custom branch..."
git checkout custom/remote-setup
git rebase main

echo "📤 Pushing updated custom branch..."
git push --force-with-lease origin custom/remote-setup

echo "✅ Sync complete!"
```

Make it executable:
```bash
chmod +x sync-upstream.sh
```

## 🐛 Handling Conflicts

If you get conflicts during rebase:

1. **Check which files have conflicts:**
   ```bash
   git status
   ```

2. **For each conflicted file, edit and resolve:**
   ```bash
   code <conflicted-file>  # or your preferred editor
   ```

3. **Mark as resolved and continue:**
   ```bash
   git add <resolved-file>
   git rebase --continue
   ```

4. **If things go wrong, abort and try again:**
   ```bash
   git rebase --abort
   ```

## 📝 Contributing Back

If you make improvements that would benefit everyone:

1. Create a new branch from `main`:
   ```bash
   git checkout main
   git checkout -b feature/my-improvement
   ```

2. Cherry-pick or re-implement your changes

3. Push and create a Pull Request to the upstream repo

---

**Remember**: Your `custom/remote-setup` branch is YOUR personal configuration. Keep it clean, rebase regularly, and your fork will stay healthy! 🚀
