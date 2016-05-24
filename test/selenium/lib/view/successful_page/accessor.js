'use strict';

const Accessor = require('../accessor');


function SuccessfulPageAccessor() {
  Accessor.apply(this, arguments);
}

SuccessfulPageAccessor.prototype = Object.assign({
  get successMessageElement() {
    return this.waitForElement('#thank-you');
  }
}, Accessor.prototype);

module.exports = SuccessfulPageAccessor;
