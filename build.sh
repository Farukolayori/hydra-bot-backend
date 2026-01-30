#!/bin/bash

# ============================================================================
# HYDRA MASTER - RUST BUILD SCRIPT
# ============================================================================

set -e  # Exit on error

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🦀 HYDRA MASTER - RUST BUILD"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo -e "${NC}"

# Check if Rust is installed
if ! command -v rustc &> /dev/null; then
    echo -e "${RED}✗ Rust is not installed!${NC}"
    echo ""
    echo "Install Rust by running:"
    echo -e "${YELLOW}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
    echo ""
    exit 1
fi

# Show Rust version
RUST_VERSION=$(rustc --version)
echo -e "${GREEN}✓ Rust detected: ${RUST_VERSION}${NC}"
echo ""

# Create .env if it doesn't exist
if [ ! -f .env ]; then
    echo -e "${YELLOW}Creating .env file from .env.rust.example...${NC}"
    cp .env.rust.example .env
    echo -e "${GREEN}✓ .env created${NC}"
    echo -e "${YELLOW}⚠  Edit .env file to configure your bot${NC}"
    echo ""
fi

# Ask for build type
echo "Select build type:"
echo "  1) Debug (fast compile, slower runtime)"
echo "  2) Release (slow compile, maximum performance) ${GREEN}[RECOMMENDED]${NC}"
echo ""
read -p "Choice [1-2]: " choice

case $choice in
    1)
        echo ""
        echo -e "${CYAN}Building in DEBUG mode...${NC}"
        cargo build
        echo ""
        echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo -e "${GREEN}✓ Build complete!${NC}"
        echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""
        echo "To run the bot:"
        echo -e "${YELLOW}cargo run${NC}"
        echo ""
        ;;
    2)
        echo ""
        echo -e "${CYAN}Building in RELEASE mode...${NC}"
        echo -e "${YELLOW}⚠  This will take 2-5 minutes on first build${NC}"
        echo ""
        cargo build --release
        echo ""
        echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo -e "${GREEN}✓ Build complete!${NC}"
        echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
        echo ""
        echo "To run the bot:"
        echo -e "${YELLOW}cargo run --release${NC}"
        echo ""
        echo "Or use the binary directly:"
        echo -e "${YELLOW}./target/release/hydra-master${NC}"
        echo ""
        
        # Show binary size
        if [ -f ./target/release/hydra-master ]; then
            SIZE=$(du -h ./target/release/hydra-master | cut -f1)
            echo -e "${CYAN}Binary size: ${SIZE}${NC}"
        fi
        ;;
    *)
        echo -e "${RED}Invalid choice${NC}"
        exit 1
        ;;
esac

echo ""
echo -e "${CYAN}Next steps:${NC}"
echo "  1. Edit .env file with your configuration"
echo "  2. Run: ${YELLOW}cargo run --release${NC}"
echo "  3. In another terminal: ${YELLOW}cd frontend && npm run dev${NC}"
echo ""
echo -e "${GREEN}Happy MEV hunting! 🚀${NC}"
echo ""