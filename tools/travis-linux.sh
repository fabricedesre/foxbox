#/bin/bash

set -ev

CURRENT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$CURRENT_PATH/travis-linux-common.sh"

_build_without_features() {
  echo '> Building foxbox without default features.'
  cargo build --no-default-features
}

_build_default() {
    echo '> Building foxbox with default features.'
    cargo build
}

build() {
  _build_without_features
  _build_default
}

lint() {
    jshint test/**/*.js \
        static/main/js/*.js static/setup/js/*.js
        # There is a minified js in static/**/shared which breaks jshint
}

_set_up_unit_tests() {
    sudo usermod -a -G netdev "$USER" # "netdev" group membership allows to set custom host name via avahi-daemon.
}

_set_up_selenium_tests() {
    sh -e /etc/init.d/xvfb start
    export DISPLAY=:99.0
    wget http://selenium-release.storage.googleapis.com/2.53/selenium-server-standalone-2.53.0.jar
    java -jar selenium-server-standalone-2.53.0.jar > /dev/null &
    sleep 5
    nvm install 4.2
    nvm use 4.2
    npm install
}

set_up_tests() {
    _set_up_unit_tests
    _set_up_selenium_tests
}

run_tests() {
    npm run test-selenium
    npm run test-integration-travis
    # TODO: Currently unit tests are executed twice. We need a way to filter out
    # tests with `cargo test` depending on where they are defined in the tree
    "$CURRENT_PATH/execute-all-rust-tests.sh"
    # Note: Cargo recompiles every dependency with dead code. That's why
    # this step is currently the last
    "$CURRENT_PATH/execute-unit-tests-with-coverage.sh"
}
