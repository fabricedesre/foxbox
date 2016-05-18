'use strict';

var SuccessfulPageAccessor = require('./accessors.js');

function SuccessfulPageView(driver) {
  this.driver = driver;
  this.accessors = new SuccessfulPageAccessor(this.driver);

  this.accessors.successMessageLocator; // Wait until message is displayed
}

SuccessfulPageView.prototype = {
  get loginMessage() {
    return this.accessors.successMessageLocator.getText();
  },

  goToSignedIn() {
    return this.driver.navigate().to('http://localhost:3331')
      .then(() => {
        const SignedInView = require('../signed_in/view');
        return new SignedInView(this.driver);
      });
  }
};

module.exports = SuccessfulPageView;
