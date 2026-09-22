#!/usr/bin/env bash
set -euo pipefail

# The hosted Jammy runner's Azure mirror can stall for hours. Replace only that
# Ubuntu endpoint, including entries in the runner's mirror list and deb822 files.
shopt -s nullglob
sources=(/etc/apt/sources.list /etc/apt/apt-mirrors.txt /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources)
for source in "${sources[@]}"; do
  if [[ -f "$source" ]]; then
    sudo sed -i \
      -e 's|http://azure.archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
      -e 's|https://azure.archive.ubuntu.com/ubuntu|https://archive.ubuntu.com/ubuntu|g' \
      "$source"
  fi
done

apt_options=(
  -o Acquire::Retries=3
  -o Acquire::http::Timeout=30
  -o Acquire::https::Timeout=30
)
# Fail on incomplete index downloads instead of proceeding with stale indexes.
sudo apt-get "${apt_options[@]}" -o APT::Update::Error-Mode=any update
sudo apt-get "${apt_options[@]}" install -y --no-install-recommends \
  libasound2-dev \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  pkg-config \
  "$@"
