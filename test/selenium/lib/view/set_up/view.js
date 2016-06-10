'use strict';

const View = require('../view');
const PASSWORDS = require('../../passwords.json');
const Alert = require('../alert.js');


function SetUpView() {
  View.apply(this, arguments);
}

SetUpView.prototype = Object.assign({

  successLogin(email, password) {
    return this._submitPassword(email, password)
      .then(() => this.instanciateNextView('successful_page'));
  },

  successSignUpFromApp(email, password) {
    return this._submitPassword(email, password)
      .then(() => this.instanciateNextView('services'));
  },

  failureLogin(email, password, confirmPassword) {
    return this._submitPassword(email, password, confirmPassword)
      .then(() => this.alert.message);
  },

  _submitPassword(email, password, confirmPassword) {
    password = password !== undefined ? password : PASSWORDS.valid;
    confirmPassword = confirmPassword !== undefined ?
      confirmPassword : password;

    return this.accessor.emailField.sendKeys(email)
      .then(() => this.accessor.passwordField.sendKeys(password))
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
