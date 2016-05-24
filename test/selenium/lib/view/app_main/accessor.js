'use strict';

const Accessor = require('../accessor');


function MainAccessor() {
  Accessor.apply(this, arguments);
}

MainAccessor.prototype = Object.assign({

  get connectToFoxBoxButton() {
    return this.waitForElement('.user-login__login-button');
  }

}, Accessor.prototype);

module.exports = MainAccessor;
