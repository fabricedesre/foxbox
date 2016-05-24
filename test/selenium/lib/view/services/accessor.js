'use strict';

const Accessor = require('../accessor');


function ServicesAccessor() {
  Accessor.apply(this, arguments);
}

ServicesAccessor.prototype = Object.assign({

  get logOutButton() {
    return this.waitForElement('.user-logout-button');
  },

}, Accessor.prototype);

module.exports = ServicesAccessor;
