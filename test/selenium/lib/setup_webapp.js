'use strict';

var webdriver = require('selenium-webdriver');
var SetUpView = require('./view/sign_up/view.js');
var SignInPageView = require('./view/sign_in/view.js');
var MainView = require('./view/app_main/view.js');

const driverBuilder = new webdriver.Builder().forBrowser('firefox');


function SetUpWebapp(url) {
  console.log('started driver', url);
  this.url = url;
  this.driver = this.driver = driverBuilder.build();
}

SetUpWebapp.prototype = {
    init: function() {
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
      return this.getSetUpView();
    },

    getSignInPage : function() {
      return new SignInPageView(this.driver);
    },

    getSetUpView : function() {
      return new SetUpView(this.driver);
    },

    getAppMainView : function() {
      return new MainView(this.driver);
    }
};

module.exports = SetUpWebapp;
