'use strict';

var By = require('selenium-webdriver').By;
var Accessors = require('../accessors');


function SignedInPageAccessor() {
  Accessors.apply(this, arguments);
}

SignedInPageAccessor.prototype = Object.assign({
  get signOutButton() {
    return this.waitForElement(By.id('signout-button'));
  }
}, Accessors.prototype);

module.exports = SignedInPageAccessor;
