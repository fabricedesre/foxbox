'use strict';

const View = require('../view');
const PASSWORDS = require('../../passwords.json');
const Alert = require('../alert.js');


function SetUpView() {
  View.apply(this, arguments);
}

SetUpView.prototype = Object.assign({

  successLogin(password) {
    return this._submitPassword(password)
      .then(() => this.instanciateNextView('successful_page'));
  },

  successSignUpFromApp(password) {
    return this._submitPassword(password)
      .then(() => this.instanciateNextView('services'));
  },

  failureLogin(password, confirmPassword) {
    return this._submitPassword(password, confirmPassword)
      .then(() => this.alert.message);
  },

  _submitPassword(password, confirmPassword) {
    password = password !== undefined ? password : PASSWORDS.valid;
    confirmPassword = confirmPassword !== undefined ?
      confirmPassword : password;

    return this.accessor.passwordField.sendKeys(password)
      .then(() => this.accessor.confirmPasswordField.sendKeys(confirmPassword))
      .then(() => this.accessor.submitButton.click());
  },

  get alert() {
    return new Alert(this.driver);
  },

  acceptAlert() {
    return this.alert.accept();
  },

}, View.prototype);

module.exports = SetUpView;
