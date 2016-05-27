#!/bin/bash

set -ex

CURRENT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_HOME="$CURRENT_PATH/.."

GECKO_DRIVER_BINARY_NAME='wires' # wires is the previous project name of geckodriver
GECKO_DRIVER_VERSION='0.7.1'
GECKO_DRIVER_FOLDER="$PROJECT_HOME/target"
GECKO_DRIVER_BINARY="$GECKO_DRIVER_FOLDER/$GECKO_DRIVER_BINARY_NAME"

# GeckoDriver must be in the path, so Selenium client can locate it
export PATH="$PATH:$GECKO_DRIVER_FOLDER"


_build_project() {
  pushd "${PROJECT_HOME}"
  cargo build
}


_install_firefox_driver() {
  local KERNEL_NAME="$(uname -s)"
  local os=''

  if [[ $KERNEL_NAME == "Darwin" ]]; then
    os='OSX'
  elif [[ $KERNEL_NAME == "Linux" ]]; then
    os='linux64' # Warning: No 32 bits binary is available
  else
    echo 'Error: Unsupported OS'
    exit 1
  fi

  curl --location --output "$GECKO_DRIVER_BINARY.gz" \
    "https://github.com/mozilla/geckodriver/releases/download/v$GECKO_DRIVER_VERSION/$GECKO_DRIVER_BINARY_NAME-$GECKO_DRIVER_VERSION-$os.gz"

  gunzip "$GECKO_DRIVER_BINARY.gz"
  chmod +x "$GECKO_DRIVER_BINARY"
}

_run_tests() {
  "${PROJECT_HOME}/node_modules/.bin/mocha" "${PROJECT_HOME}/test/selenium/sessions_ui_test.js"
}


_build_project
if ! [ -f "$GECKO_DRIVER_BINARY" ] ; then
  _install_firefox_driver
fi
_run_tests
