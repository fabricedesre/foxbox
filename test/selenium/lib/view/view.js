'use strict';

const stack = require('callsite');
const path = require('path');

function View(driver) {
  this.driver = driver;

  // Here we fetch `accessor.js` located in the same directory than
  // the child view
  var lastCallerInStackStace = stack()[1]; // 0 actually points to this line
  var childViewDirectory = path.dirname(lastCallerInStackStace.getFileName());
  const ChildAccessor = require(childViewDirectory + '/accessor.js');
  this.accessor = new ChildAccessor(this.driver);
}

View.prototype = {
  instanciateNextView(viewFolderName) {
    const NextView = require('./' + viewFolderName + '/view.js');
    return new NextView(this.driver);
  }
};

module.exports = View;
