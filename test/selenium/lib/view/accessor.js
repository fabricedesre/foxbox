'use strict';

const webdriver = require('selenium-webdriver');
const By = webdriver.By;
const until = webdriver.until;

function Accessor(driver) {
  this.driver = driver;
}

Accessor.prototype = {
  /**
   * Waits until the element is present *and* displayed.
   * @param {string | By} locator - Either the css selector (if string) or a
   *   "Webdriver By" object.
   * @return WebElement
   */
  waitForElement(locator) {
    locator = typeof locator === 'string' ? By.css(locator) : locator;

    var element = this.driver.wait(until.elementLocated(locator));
    return this.driver.wait(until.elementIsVisible(element));
  }

};

module.exports = Accessor;
