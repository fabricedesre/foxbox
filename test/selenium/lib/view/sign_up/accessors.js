'use strict';

var webdriver = require('selenium-webdriver');
var By = require('selenium-webdriver').By;

var SELECTORS = Object.freeze({
  // Make sure these field are not plain text
  passwordField: By.css('#signup-pwd1[type="password"]'),
  confirmPasswordField: By.css('#signup-pwd2[type="password"]'),

  submit: By.id('signup-button'),
  successMessage: By.id('thank-you'),
  page: By.id('signup')
});

function SetUpAccessor(driver) {
  this.driver = driver;
}

SetUpAccessor.prototype = {
    get root() {
    var signUpPage = this.driver.findElement(SELECTORS.page);
    return this.driver.wait(webdriver.until.elementIsVisible(signUpPage));
   },

   get isPasswordFieldPresent() {
        return this.driver.isElementPresent(SELECTORS.passwordField);
   },

   get isConfirmPasswordFieldPresent() {
        return this.driver.isElementPresent(SELECTORS.confirmPasswordField);
   },

   get isSubmitButtonPresent() {
        return this.driver.isElementPresent(SELECTORS.submit);
   },

   get insertPassword() {
        return this.driver.findElement(SELECTORS.passwordField);
    },

    get confirmPassword() {
        return this.driver.findElement(SELECTORS.confirmPasswordField);
    },

    get submitButton() {
        return this.driver.findElement(SELECTORS.submit);
    },

   get successMessageLocator() {
        return this.driver.findElement(SELECTORS.successMessage);
   }
};

module.exports = SetUpAccessor;
