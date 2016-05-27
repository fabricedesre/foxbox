'use strict';

const until = require('selenium-webdriver').until;


function AlertWrapper(driver) {
  this.driver = driver;
}

AlertWrapper.prototype = {

  get message() {
    return this._waitForAlert().then(alert => alert.getText());
  },

  accept() {
    return this._waitForAlert().then(alert => alert.accept());
  },

  _waitForAlert() {
    return this.driver.wait(until.alertIsPresent());
  },
};

module.exports = AlertWrapper;
