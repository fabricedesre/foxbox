'use strict';

const Accessor = require('../accessor');


function SignedInPageAccessor() {
  Accessor.apply(this, arguments);
}

SignedInPageAccessor.prototype = Object.assign({
  get signOutButton() {
    return this.waitForElement('#signout-button');
  }
}, Accessor.prototype);

module.exports = SignedInPageAccessor;
