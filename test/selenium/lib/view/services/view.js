'use strict';

const View = require('../view');


function ServicesView() {
  View.apply(this, arguments);

  this.accessor.logOutButton;  // Wait until it appears
}

module.exports = ServicesView;
