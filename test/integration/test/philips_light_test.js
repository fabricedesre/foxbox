'use strict';

const chakram = require('chakram'), expect = chakram.expect;

var Prepper = require('../lib/make_suite.js');

Prepper.makeSuite('Control lights locally',function(){

  var getterPayload = [{'feature': 'light/is-on'}];
  var lights;

  before(Prepper.turnOnAllSimulators);
  before(Prepper.turnOnFoxbox);
  before(Prepper.foxboxManager.foxboxLogin);

  it('check 3 bulbs are registered',function(){
  
    // collect all getters for the lightbulbs
    return chakram.put(Prepper.foxboxManager.getterURL,getterPayload)
    .then(function(listResponse) {
      lights = Object.keys(listResponse.body);
      expect(lights.length).equals(3);
      expect(listResponse).to.have.status(200);
    });
  });

  describe('manipulate lights', function () {
    before(function() {
      // turn off all lights in the simulator
      Prepper.philipshue_server.turnOffAllLights([1,2,3]);

      return chakram.put(Prepper.foxboxManager.getterURL,getterPayload)
      .then(function(listResponse) {
        lights = Object.keys(listResponse.body);
        expect(lights.length).equals(3);
        expect(listResponse).to.have.status(200);
      });
    });

    // Currently, there is no mapping between the foxbox
    // id and the philips hue id until the tag feature is implemented
    it('Turn on all lights one by one', function(){
      var lightID = lights[0];
      var payload = {'select': {'id': lightID}, 'value': 'On' };

      return chakram.put(Prepper.foxboxManager.setterURL,payload)
      .then(function(cmdResponse) {
       expect(cmdResponse).to.have.status(200);
       expect(cmdResponse.body[lightID]).equals(null);
       lightID = lights[1];
       payload = {'select': {'id': lightID}, 'value': 'On' };
       return chakram.put(Prepper.foxboxManager.setterURL,payload);
     })
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse.body[lightID]).equals(null);
        lightID = lights[2];
        payload = {'select': {'id': lightID}, 'value': 'On' };
        return chakram.put(Prepper.foxboxManager.setterURL,payload);
      })
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse.body[lightID]).equals(null);
        expect(Prepper.philipshue_server.areAllLightsOn()).to.be.true;

        //check all lights are reported to be on
        return chakram.put(Prepper.foxboxManager.getterURL,getterPayload);
      })
      .then(function(listResponse) {
        expect(listResponse).to.have.status(200);
        lights = Object.keys(listResponse.body);
        expect(lights.length).equals(3);

        lights.forEach(light => {
          expect(listResponse.body[light]).equals('On');
        });
      }); 
    });

    it('Turn off all lights at once', function() {

      var payload = {'select': {'feature': 'light/is-on'},
        'value': 'Off'};

      return chakram.put(Prepper.foxboxManager.setterURL,payload)
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        expect(Prepper.philipshue_server.areAllLightsOn()).to.be.false;

        //check all lights are reported to be off
        return chakram.put(Prepper.foxboxManager.getterURL,getterPayload);
      })
      .then(function(listResponse) {
        lights = Object.keys(listResponse.body);
        expect(lights.length).equals(3);
        expect(listResponse).to.have.status(200);

        lights.forEach(light => {
          expect(listResponse.body[light]).equals('Off');
        });
      }); 
    });
  });
});
