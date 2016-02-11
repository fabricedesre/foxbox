/* Any copyright is dedicated to the Public Domain.
 * http://creativecommons.org/publicdomain/zero/1.0/ */

'use strict';

var webdriver = require('selenium-webdriver'),
  assert = require('assert');

describe('setup page', function() {
  var driver;
  this.timeout(8000);
  before(function() {
    driver = new webdriver.Builder().
      forBrowser('firefox').
      build();
  });
  beforeEach(function() {
    driver.get('http://foxbox.local:3000/');
  });
  after(function() {
    driver.quit();
  });

  it('should be titled FoxBox', function () {
    return driver.wait(webdriver.until.titleIs('FoxBox'), 5000)
      .then(function(value) {
        assert.equal(value, true);
      });
  });
  describe('UI to set admin password', function() {
    var elts;
    beforeEach(function() {
      elts = {
        pwd1: driver.findElement(webdriver.By.id('pwd1-input')),
        pwd2: driver.findElement(webdriver.By.id('pwd2-input')),
        set: driver.findElement(webdriver.By.id('set-button'))
      };
    });
    it('should have the right fields', function () {
      var types = {
        pwd1: 'password',
        pwd2: 'password',
        set: 'submit'
      };
      var promises = Object.keys(elts).map(function(key) {
        return elts[key].getAttribute('type').then(function(value) {
          assert.equal(value, types[key]);
        });
      });
      return Promise.all(promises);
    });

    it('should reject non-matching passwords', function () {
      return elts.pwd1.sendKeys('asdfasdf').then(function() {
        return elts.pwd2.sendKeys('qwerqwer');
      }).then(function() {
        return elts.set.click();
      }).then(function() {
        return driver.wait(webdriver.until.alertIsPresent(), 5000);
      }).then(function() {
        return driver.switchTo().alert();
      }).then(function(alert) {
        return alert.getText().then(function(text) {
          assert.equal(text, 'Passwords don\'t match! Please try again.');
        }).then(function() {
          alert.dismiss();
        });
      });
    });

    it('should reject short passwords', function () {
      return elts.pwd1.sendKeys('asdf').then(function() {
        return elts.pwd2.sendKeys('asdf');
      }).then(function() {
        return elts.set.click();
      }).then(function() {
        return driver.wait(webdriver.until.alertIsPresent(), 5000);
      }).then(function() {
        return driver.switchTo().alert();
      }).then(function(alert) {
        return alert.getText().then(function(text) {
          assert.equal(text, 'Please use a password of at least 8 characters.');
        }).then(function() {
          alert.dismiss();
        });
      });
    });

    it('should accept matching, long-enough passwords', function () {
      return elts.pwd1.sendKeys('asdfasdf').then(function() {
        return elts.pwd2.sendKeys('asdfasdf');
      }).then(function() {
        return elts.set.click();
      }).then(function() {
        return driver.findElement(webdriver.By.id('thank-you'));
      }).then(function(elt) {
        return driver.wait(webdriver.until.elementIsVisible(elt), 5000)
          .then(function() {
            return elt.getAttribute('innerHTML');
          }).then(function(value) {
            assert.equal(value, 'Thank you!');
          });
      });
    });
  });
});
