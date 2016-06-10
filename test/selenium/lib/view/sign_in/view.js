'use strict';

const View = require('../view');
const PASSWORDS = require('../../passwords.json');
const Alert = require('../alert.js');


function SignInView() {
  View.apply(this, arguments);
}

SignInView.prototype = Object.assign({
  successLogin(email, password) {
    password = password !== undefined ? password : PASSWORDS.valid;
    return this._submitPassword(email, password)
      .then(() => this.instanciateNextView('signed_in'));
  },

  failureLogin(email, password) {
    return this._submitPassword(email, password)
    .then(() => new Alert(this.driver).message);
  },

  _submitPassword(email, password) {
    return this.accessor.emailField.sendKeys(email)
      .then(() => this.accessor.passwordField.sendKeys(password))
      .then(() => this.accessor.submitButton.click());
  },

}, View.prototype);

module.exports = SignInView;
