# Local Development Setup

This guide covers setting up a complete CipherOcto development environment on your local machine.

---

## System Requirements

### Minimum Requirements

| Component | Minimum |
| --------- | -------- |
| **OS** | Linux (Ubuntu 22.04+), macOS 13+, Windows 11 with WSL2 |
| **CPU** | 4 cores, x86_64 or arm64 |
| **RAM** | 8 GB |
| **Storage** | 20 GB free space |
| **Network** | Stable internet connection |

### Recommended Requirements

| Component | Recommended |
| ----------- | ----------- |
| **OS** | Ubuntu 22.04 LTS or macOS 14+ |
| **CPU** | 8+ cores |
| **RAM** | 16 GB+ |
| **Storage** | 50 GB+ SSD |
| **GPU** | NVIDIA GPU (compute capability 7.0+) with 8GB+ VRAM |
| **Network** | 100 Mbps+ connection |

---

## Prerequisites Installation

### Step 1: Install Core Dependencies

#### Linux (Ubuntu/Debian)

```bash
# Update package list
sudo apt update

# Install build essentials
sudo apt install -y build-essential git curl wget

# Install Node.js 18+
curl -fsSL https://deb.nodesource.com/setup_18.x | sudo -E bash -
sudo apt install -y nodejs

# Install Python 3.10+
sudo apt install -y python3 python3-pip python3-venv

# Install Rust (optional, for core development)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

#### macOS

```bash
# Install Homebrew if not already installed
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install dependencies
brew install node@18 python@3.11 git rust

# Add to PATH (add to ~/.zshrc or ~/.bash_profile)
export PATH="/opt/homebrew/opt/node@18/bin:$PATH"
export PATH="/opt/homebrew/opt/python@3.11/bin:$PATH"
```

#### Windows (WSL2)

```powershell
# Enable WSL2
wsl --install

# After restart, open Ubuntu terminal and follow Linux instructions
```

### Step 2: Install CUDA (for GPU support)

**NVIDIA GPU only**

```bash
# Ubuntu 22.04
wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-keyring_1.1-1_all.deb
sudo dpkg -i cuda-keyring_1.1-1_all.deb
sudo apt update
sudo apt install -y cuda-toolkit-12-2 cuda-12-2

# Add to PATH
echo 'export PATH=/usr/local/cuda-12.2/bin:$PATH' >> ~/.bashrc
echo 'export LD_LIBRARY_PATH=/usr/local/cuda-12.2/lib64:$LD_LIBRARY_PATH' >> ~/.bashrc
source ~/.bashrc
```

Verify installation:

```bash
nvcc --version
nvidia-smi
```

### Step 3: Install Docker (optional)

```bash
# Linux
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh
sudo usermod -aG docker $USER

# macOS
brew install --cask docker

# Start Docker Desktop and complete setup
```

---

## Repository Setup

### Clone and Configure

```bash
# Clone the repository
git clone https://github.com/cipherocto/cipherocto.git
cd cipherocto

# Install Node.js dependencies
npm install

# Install Python dependencies
pip install -e .

# Install Rust dependencies (optional)
cd rust && cargo build && cd ..
```

### Repository Structure

```
cipherocto/
├── packages/
│   ├── sdk/              # TypeScript SDK
│   ├── cli/              # Command-line interface
│   └── agent/            # Agent framework
├── contracts/            # Smart contracts
├── rust/                 # Core Rust implementation
├── python/               # Python SDK
├── docs/                 # Documentation
├── examples/             # Example code
└── tests/                # Test suites
```

---

## Development Environment Setup

### 1. Configure CLI

```bash
# Initialize configuration
cipherocto config init

# Set development environment
cipherocto config set environment development

# Configure local node
cipherocto config set rpc-url http://localhost:8545
cipherocto config set chain-id 1337
```

### 2. Start Local Testnet

```bash
# Start local blockchain node
npm run node:start

# In another terminal, deploy contracts
npm run contracts:deploy

# Verify deployment
cipherocto network status
```

### 3. Fund Development Wallet

```bash
# Create or import wallet
cipherocto wallet create --name dev
cipherocto wallet import --private-key <your-key>

# Fund from local faucet (localhost only)
cipherocto faucet request

# Check balance
cipherocto wallet balance
```

---

## IDE Setup

### VS Code

**Recommended Extensions:**

```bash
# Install extensions
code --install-extension dbaeumer.vscode-eslint
code --install-extension esbenp.prettier-vscode
code --install-extension ms-python.python
code --install-extension rust-lang.rust-analyzer
code --install-extension bradlc.vscode-tailwindcss
code --install-extension usernamehw.errorlens
```

**Workspace Configuration:**

Create `.vscode/settings.json`:

```json
{
  "typescript.tsdk": "node_modules/typescript/lib",
  "eslint.validate": ["typescript", "typescriptreact"],
  "editor.formatOnSave": true,
  "editor.defaultFormatter": "esbenp.prettier-vscode",
  "python.linting.enabled": true,
  "python.linting.pylintEnabled": true,
  "rust-analyzer.checkOnSave.command": "clippy"
}
```

### JetBrains IDEs

**IntelliJ IDEA / PyCharm / GoLand:**

1. Install **LSP Support** plugin
2. Install **Rust** plugin (for Rust development)
3. Configure Python interpreter for Python SDK
4. Enable **ESLint** and **Prettier** for TypeScript

---

## Running Tests

### Unit Tests

```bash
# TypeScript/JavaScript
npm test

# Python
pytest tests/

# Rust
cargo test
```

### Integration Tests

```bash
# Start local environment
npm run test:setup

# Run integration tests
npm run test:integration

# Cleanup
npm run test:cleanup
```

### Test Coverage

```bash
# Generate coverage report
npm run test:coverage

# View report
open coverage/index.html
```

---

## Local Agent Development

### Create Your First Agent

```bash
# Create new agent project
cipherocto agent init my-first-agent

# Navigate to agent directory
cd my-first-agent

# Start development server
npm run dev
```

### Agent Project Structure

```
my-first-agent/
├── src/
│   ├── index.ts          # Agent entry point
│   ├── handlers.ts       # Task handlers
│   └── config.ts         # Agent configuration
├── tests/
│   └── agent.test.ts     # Agent tests
├── package.json
└── tsconfig.json
```

### Development Workflow

```bash
# Watch for changes and rebuild
npm run watch

# Run agent locally
npm run start

# Test agent with mock tasks
npm run test:mock

# Build for production
npm run build

# Deploy to local testnet
npm run deploy:local
```

---

## Local Node Operation

### Run a Validator Node

```bash
# Initialize node
cipherocto node init --data-dir ~/.cipherocto/node

# Generate validator key
cipherocto node key generate --type validator

# Start node
cipherocto node start

# Check node status
cipherocto node status
```

### Run a Provider Node

```bash
# Register as provider
cipherocto provider register --type gpu

# Verify hardware
cipherocto provider verify --gpu

# Start provider
cipherocto provider start

# Monitor logs
cipherocto provider logs --follow
```

---

## Docker Development

### Development Container

```bash
# Build development image
docker build -t cipherocto:dev -f docker/Dockerfile.dev .

# Run container
docker run -it --rm \
  -v $(pwd):/workspace \
  -p 8545:8545 \
  cipherocto:dev bash
```

### Docker Compose

```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down
```

---

## Troubleshooting

### Issue: Port Already in Use

```bash
# Find process using port
lsof -i :8545

# Kill process
kill -9 <PID>

# Or use different port
cipherocto config set rpc-url http://localhost:8546
```

### Issue: Node.js Version Incompatibility

```bash
# Use nvm to manage Node versions
nvm install 18
nvm use 18

# Verify version
node --version  # Should be v18.x.x
```

### Issue: Python Import Errors

```bash
# Create virtual environment
python3 -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate

# Reinstall dependencies
pip install -e .
```

### Issue: CUDA Not Found

```bash
# Check CUDA installation
nvcc --version

# Update PATH if needed
export PATH=/usr/local/cuda/bin:$PATH
export LD_LIBRARY_PATH=/usr/local/cuda/lib64:$LD_LIBRARY_PATH
```

---

## Development Tips

### Hot Reload

```bash
# Enable hot reload for agent development
npm run dev -- --watch
```

### Debug Logging

```bash
# Enable debug logging
export DEBUG=cipherocto:*
cipherocto agent start
```

### Fast Iteration

```bash
# Skip build step for faster testing
node --loader ts-node/esm src/index.ts
```

---

## Environment Variables

| Variable | Description | Default |
| ---------- | ----------- | ------- |
| `CIHEROCTO_RPC_URL` | RPC endpoint | http://localhost:8545 |
| `CIHEROCTO_CHAIN_ID` | Chain ID | 1337 |
| `CIHEROCTO_PRIVATE_KEY` | Wallet private key | — |
| `CIHEROCTO_DATA_DIR` | Data directory | ~/.cipherocto |
| `DEBUG` | Debug logging | — |
| `NODE_ENV` | Environment | development |

---

## Next Steps

1. **Build your first agent** — Follow [getting-started.md](./getting-started.md)
2. **Explore examples** — Check [examples/](https://github.com/cipherocto/examples)
3. **Run tests** — Ensure your environment works: `npm test`
4. **Join community** — Get help: [discord.gg/cipherocto](https://discord.gg/cipherocto)

---

**Happy coding! 🐙**

---

*For contribution guidelines, see [contributing.md](./contributing.md).*
