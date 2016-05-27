'use strict';

const webdriver = require('selenium-webdriver');
const SetUpView = require('./view/set_up/view.js');
const SignInPageView = require('./view/sign_in/view.js');
const MainView = require('./view/app_main/view.js');
var firefoxCapabilities = require('selenium-webdriver/lib/capabilities')
  .Capabilities.firefox();

firefoxCapabilities.set('marionette', true);
const driverBuilder = new webdriver.Builder()
  .withCapabilities(firefoxCapabilities);


function SetUpWebapp(url) {
  this.url = url;
  this.driver = driverBuilder.build();
}

SetUpWebapp.prototype = {
  init() {
    return this.driver.get(this.url)
      .then(() => this.defaultView);
  },

  clear() {
    // Session data is not stored in cookies, but in local storage
    return this._clearLocalStorage()
      .then(() => this.init());
  },

  _clearLocalStorage() {
    return this.driver.executeScript('localStorage.clear();');
  },

  stop() {
    return this.driver.quit()
      .then(() => { this.driver = null; });
  },

  get defaultView() {
    return this.setUpView;
  },

  get signInPage() {
    return new SignInPageView(this.driver);
  },

  get setUpView() {
    return new SetUpView(this.driver);
  },

  get appMainView() {
    return new MainView(this.driver);
  }
};

module.exports = SetUpWebapp;
