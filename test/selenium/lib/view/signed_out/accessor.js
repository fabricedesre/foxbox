'use strict';

const Accessor = require('../accessor');


function SignedOutAccessor() {
  Accessor.apply(this, arguments);
}

SignedOutAccessor.prototype = Object.assign({
  get root() {
    return this.waitForElement('#signin');
  }
}, Accessor.prototype);

module.exports = SignedOutAccessor;
