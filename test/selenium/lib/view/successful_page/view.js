'use strict';

const View = require('../view');


function SuccessfulPageView() {
  View.apply(this, arguments);

  this.accessor.successMessageElement; // Wait until message is displayed
}

SuccessfulPageView.prototype = Object.assign({
  get loginMessage() {
    return this.accessor.successMessageElement.getText();
  },

  goToSignedIn() {
    return this.driver.navigate().to('http://localhost:3331')
      .then(() => this.instanciateNextView('signed_in'));
  }
}, View.prototype);

module.exports = SuccessfulPageView;
