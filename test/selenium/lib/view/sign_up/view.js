'use strict';

var SetUpAccessor = require('./accessors.js');


function SetUpView(driver) {
    this.driver = driver;
    this.accessors = new SetUpAccessor(this.driver);
}

SetUpView.prototype = {
    isSetUpView: function() {
        return this.accessors.root;
    },

    passwordField: function() {
        return this.accessors.isPasswordFieldPresent;
    },

    passwordConfirmField: function() {
        return this.accessors.isConfirmPasswordFieldPresent;
    },

    submitButtonPresent: function() {
        return this.accessors.isSubmitButtonPresent;
    },

    successLogin: function(password) {
      return this._submitPassword(password)
        .then(() => {
          const SuccessfulPageView = require('../successful_page/view.js');
          return new SuccessfulPageView(this.driver);
        });
    },

    successSignUpFromApp: function(password) {
      return this._submitPassword(password)
        .then(() => {
          var ServicesView = require('../services/view');
          return new ServicesView(this.driver);
        });
    },

    failureLogin: function(password, confirmPassword) {
      return this._submitPassword(password, confirmPassword)
        .then(() => this.alertMessage());
    },

    _submitPassword: function(password, confirmPassword) {
      password = password !== undefined ? password : 12345678;
      confirmPassword = confirmPassword !== undefined ?
        confirmPassword : password;

      return this.accessors.insertPassword.sendKeys(password)
        .then(() => this.accessors.confirmPassword.sendKeys(confirmPassword))
        .then(() => this.accessors.submitButton.click());
    },

    alertMessage: function() {
        return this.driver.switchTo().alert().getText();
    },

    dismissAlert: function() {
       return this.driver.switchTo().alert().accept();
    },

};

module.exports = SetUpView;
