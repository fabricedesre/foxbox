'use strict';

var By = require('selenium-webdriver').By;
var Accessors = require('../accessors');

function SignInAccessor() {
  Accessors.apply(this, arguments);
}

SignInAccessor.prototype = Object.assign({

  get password() {
    // Make sure this field is not plain text
    return this.waitForElement(By.css('#signin-pwd[type="password"]'));
  },

  get submitButton() {
    return this.waitForElement(By.id('signin-button'));
  }
}, Accessors.prototype);

module.exports = SignInAccessor;
