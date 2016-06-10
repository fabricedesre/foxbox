'use strict';

const Accessor = require('../accessor');


function SetUpAccessor() {
  Accessor.apply(this, arguments);
}

SetUpAccessor.prototype = Object.assign({
  get emailField() {
    return this.waitForElement('#signup-email');
  },

  get passwordField() {
    // This makes sure password field is not plain text
    return this.waitForElement('#signup-pwd1[type="password"]');
  },

  get confirmPasswordField() {
    return this.waitForElement('#signup-pwd2[type="password"]');
  },

  get submitButton() {
    return this.waitForElement('#signup-button');
  },

}, Accessor.prototype);

module.exports = SetUpAccessor;
