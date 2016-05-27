'use strict';

const View = require('../view');
const PASSWORDS = require('../../passwords.json');
const Alert = require('../alert.js');


function SignInView() {
  View.apply(this, arguments);
}

SignInView.prototype = Object.assign({
  successLogin(password) {
    password = password !== undefined ? password : PASSWORDS.valid;
    return this._submitPassword(password)
      .then(() => this.instanciateNextView('signed_in'));
  },

  failureLogin(password) {
    return this._submitPassword(password)
    .then(() => new Alert(this.driver).message);
  },

  _submitPassword(password) {
    return this.accessor.passwordField.sendKeys(password)
      .then(() => this.accessor.submitButton.click());
  },

}, View.prototype);

module.exports = SignInView;
