'use strict';

var SignedInPageAccessor = require('./accessors.js');
var SignedOutPageView = require('../signed_out/view.js');

var signedOutPageView;

function SignedInPageView(driver) {
    this.driver = driver;
    this.accessors = new SignedInPageAccessor(this.driver);
    signedOutPageView = new SignedOutPageView(this.driver);
    this.accessors.signOutButton; // Wait until button is displayed
}

SignedInPageView.prototype = {
  signOut: function() {
    return this.accessors.signOutButton.click()
    .then(() => signedOutPageView.hasSignedOut());
  }
};

module.exports = SignedInPageView;
