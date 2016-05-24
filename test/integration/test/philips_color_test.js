'use strict';

const chakram = require('chakram'), expect = chakram.expect;

var Prepper = require('../lib/make_suite.js');

function translateHSB(h,s,b) {
  var transHue = parseInt(65536 * (h % 360 / 360));
  var transSat = parseInt(254 * s);
  var transBri = parseInt(254 * b);

  return {'h':transHue,'s':transSat,'b':transBri};
}

Prepper.makeSuite('Control lights locally',function(){

  var lights;
  var getterPayload = [{'kind': 'LightOn'}];

  before(Prepper.turnOnAllSimulators);
  before(Prepper.turnOnFoxbox);
  before(Prepper.foxboxManager.foxboxLogin);

  // collect all getters for the lightbulbs
  before(function(){
    return chakram.put(Prepper.foxboxManager.getterURL,getterPayload)
    .then(function(listResponse) {
      expect(listResponse).to.have.status(200);
      lights = Object.keys(listResponse.body);
      expect(Prepper.philipshue_server.areAllLightsOn()).equals(true);
    });
  });

  var testParams = [{testName:'upper boundary',
  lightID:0, hue: 359, sat:0.9, bri:0.9}, 
  {testName:'middle', lightID:1, hue: 200, sat:0.5, bri:0.5},
  {testName:'lower boundary', lightID:2, hue: 0, sat:0.1, bri:0.1},
  ];

  testParams.forEach(testParam => {
    it(testParam.testName, function() {
    
      lights[testParam.lightID] = 
      lights[testParam.lightID].replace('getter:power','setter:color');
      var hue = testParam.hue;
      var sat = testParam.sat;
      var bri = testParam.bri;
      
      // send various values within valid range
      var payload = {'select': {'id': lights[testParam.lightID]}, 
      'value': { 'Color': {'h':hue,'s':sat,'v':bri} } };

      return chakram.put(Prepper.foxboxManager.setterURL,payload)
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        var hsb = Prepper.philipshue_server.
        getHSB(Prepper.philipshue_server.lastLight());
        var trans = translateHSB(hue,sat,bri);
        expect(hsb.h).equals(trans.h);
        expect(hsb.s).equals(trans.s);
        expect(hsb.b).equals(trans.b);             
      });
    });
  });

  it('Send invalid sat value', function() {   
    lights[0] = lights[0].replace('getter:power','setter:color');
    var hue = 200;
    var invalidSat = 1.5;  // range = [0,1]
    var bri = 0.5;

    // send various values within valid range
    var payload = {'select': {'id': lights[0]}, 
      'value': { 'Color': {'h':hue,'s':invalidSat,'v':bri} } };

    return chakram.put(Prepper.foxboxManager.setterURL,payload)
    .then(function(cmdResponse) {
      expect(cmdResponse).to.have.status(200);
      expect(cmdResponse.body[lights[0]].Error.ParseError.TypeError.expected)
        .equals("a number in [0, 1]");
      expect(cmdResponse.body[lights[0]].Error.ParseError.TypeError.name)
        .equals("s");
    });
  });

  it('Send invalid brightness value', function() {   
    lights[1] = lights[1].replace('getter:power','setter:color');
    var hue = 200;
    var sat = 0.5;  // range = [0,1]
    var invalidBri = 1.5;  // Value should be less than or equal to 1

    // send various values within valid range
    var payload = {'select': {'id': lights[1]}, 
      'value': { 'Color': {'h':hue,'s':sat,'v':invalidBri} } };

    return chakram.put(Prepper.foxboxManager.setterURL,payload)
    .then(function(cmdResponse) {
      expect(cmdResponse).to.have.status(200);
      expect(cmdResponse.body[lights[1]].Error.ParseError.TypeError.expected)
        .equals("a number in [0, 1]");
      expect(cmdResponse.body[lights[1]].Error.ParseError.TypeError.name)
        .equals("v");
    });
  });
});
