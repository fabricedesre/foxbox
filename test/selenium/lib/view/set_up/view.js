'use strict';

const View = require('../view');
const PASSWORDS = require('../../passwords.json');


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
      .then(() => this.alertMessage());
  },

  _submitPassword(password, confirmPassword) {
    password = password !== undefined ? password : PASSWORDS.valid;
    confirmPassword = confirmPassword !== undefined ?
      confirmPassword : password;

    return this.accessor.passwordField.sendKeys(password)
      .then(() => this.accessor.confirmPasswordField.sendKeys(confirmPassword))
      .then(() => this.accessor.submitButton.click());
  },

  alertMessage() {
    return this.driver.switchTo().alert().getText();
  },

  dismissAlert() {
    return this.driver.switchTo().alert().accept();
  },

}, View.prototype);

module.exports = SetUpView;
