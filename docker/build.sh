#!/bin/bash
set -e
cd "$(dirname "$0")"
if ! [ -x rustup.sh ]; then
  curl -f https://sh.rustup.rs -o rustup.sh
  chmod +x rustup.sh
fi
docker build . -t shyper-runner:latest "$@"
