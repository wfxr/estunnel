#!/usr/bin/env bash

set -ex

if [ "$TRAVIS_OS_NAME" != linux ]; then
    exit 0
fi

sudo apt update

sudo apt install -y jq

PROJECT_NAME=$(cargo metadata --no-deps --format-version=1 | jq -r '.packages[0].name')
export PROJECT_NAME

# needed for i686 linux gnu target
if [[ $TARGET == i686-unknown-linux-gnu ]]; then
    sudo apt install -y gcc-multilib
fi
