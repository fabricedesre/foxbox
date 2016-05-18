'use strict';

var SignInAccessor = require('./accessors.js');
var View = require('../view');

function SignInView(driver) {
  [].push.call(arguments, SignInAccessor);
  View.apply(this, arguments);
}

SignInView.prototype = {
  successLogin: function(password) {
    password = password !== undefined ? password : 12345678;
    return this._submitPassword(password)
    .then(() => {
      const SignedInView = require('../signed_in/view.js');
      return new SignedInView(this.driver);
    });
  },

  failureLogin: function(password) {
    return this._submitPassword(password)
    .then(() => this.alertMessage());
  },

  _submitPassword: function(password) {
    return this.accessors.password.sendKeys(password)
    .then(() => this.accessors.submitButton.click());
  },

  alertMessage: function() {
    return this.driver.switchTo().alert().getText();
  },

  dismissAlert: function() {
    return this.driver.switchTo().alert().accept();
  },
};

module.exports = SignInView;
