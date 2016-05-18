'use strict';

const FoxboxProcessManager = require('./foxbox_process_manager');
const App = require('./setup_webapp');

function Suite(description, hostUrl) {
  hostUrl = hostUrl || FoxboxProcessManager.HOST_URL;
  this.app = new App(hostUrl);

  this.description = description;
  this.foxboxProcessManager = new FoxboxProcessManager();
}

Suite.prototype = {
  build(subSuite) {
    var self = this;

    describe(this.description, function() {
      this.timeout(30000);

      before(() => {
        return self.foxboxProcessManager.start()
        .then(() => self.app.init());
      });

      subSuite(self.app);

      after(() => {
        return self.app.stop()
        .then(() => self.foxboxProcessManager.kill())
        .then(() => self.foxboxProcessManager.cleanData());
      });
    });
  },

  browserCleanUp() {
    return this.app.clear()
    // init() should run even if clear() failed. This is useful at the initial
    // start up, when there is nothing to clear
    .then(() => this.app.init(),
          () => this.app.init());
  },

  restartFromScratch() {
    return this.app.clear()
    .then(() => this.foxboxProcessManager.kill())
    .then(() => this.foxboxProcessManager.cleanData())
    .then(() => this.foxboxProcessManager.start())
    .then(() => this.app.init());
  }
};


module.exports = Suite;
