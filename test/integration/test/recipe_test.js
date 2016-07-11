'use strict';

const chakram = require('chakram'), expect = chakram.expect;
var Prepper = require('../lib/make_suite.js');

var source = {'name': 'Hellooo, Thinkerbell', 'rules':
          [{'conditions':
            [{'source':
              [{'id':'OpenZWave72057594126794752 (Sensor)'}],
            'feature':'door/is-open',
            'when':{'Eq': 'Open'}
          }],
          'execute':
            [{'destination':
              [{'feature':'log/append-text'}],
              'feature':'log/append-text',
              'value': 'Closed'}]}
            ]};

function cloneRecipe(newRecipeName) {
  let clonedRecipe = JSON.parse(JSON.stringify(source));
  clonedRecipe.name = newRecipeName;
  return clonedRecipe;
}

function generateThinkerbellNewRecipePayload(recipeName) {
  return {
    'select':{
       'feature':'thinkerbell/add-rule'
    },
    'value': cloneRecipe(recipeName)
  };
}

function generateThinkerbellDeletionPayload(recipeName) {
  return {'select':
      {'id': 'thinkerbell/'+recipeName+'/remove', 'feature': 'thinkerbell/remove-rule-id'},'value':null};
}

function generateThinkerbellGetEnabledPayload(recipeName) {
  return { 'id': 'thinkerbell/' + recipeName + '/is-rule-enabled', 'feature': 'thinkerbell/is-rule-enabled' };
}

function generateThinkerbellSourcePayload(recipeName) {
  return { 'id': 'thinkerbell/' + recipeName + '/source', 'feature': 'thinkerbell/rule-source' };
}

Prepper.makeSuite('Add/Remove/Get recipe',function(){

  before(Prepper.turnOnAllSimulators);
  before(Prepper.turnOnFoxbox);
  before(Prepper.foxboxManager.foxboxLogin);

  it('Check service list for recipe',function(){
    return chakram.get(Prepper.foxboxManager.serviceListURL)
    .then(function(listResponse) {
      expect(listResponse).to.have.status(200);
      var isFound = listResponse.body.some(
        entry => entry.adapter === 'thinkerbell@link.mozilla.org');
      expect(isFound).to.be.true;
    });
  });

  describe('add recipes', function(){
    it('Add recipe',function(){
    return chakram.put(Prepper.foxboxManager.setterURL,
      generateThinkerbellNewRecipePayload('First Recipe'))
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse.body['thinkerbell-add-rule']).equals(null);
        return chakram.get(Prepper.foxboxManager.serviceListURL);
      })
      .then(function(listResponse) {
        expect(listResponse).to.have.status(200);
        var isFound = listResponse.body.some(
          entry => entry.id === 'thinkerbell/First Recipe');
        expect(isFound).to.be.true;
      });
    });

    it('Add two recipes one by one',function(){
      return chakram.put(Prepper.foxboxManager.setterURL,
        generateThinkerbellNewRecipePayload('Second Recipe'))
        .then(function(cmdResponse) {

          expect(cmdResponse).to.have.status(200);
          expect(cmdResponse.body['thinkerbell-add-rule']).equals(null);
          return chakram.put(Prepper.foxboxManager.setterURL,
            generateThinkerbellNewRecipePayload('Third Recipe'));
        })
        .then(function(cmdResponse) {
          expect(cmdResponse).to.have.status(200);
          expect(cmdResponse.body['thinkerbell-add-rule']).equals(null);
          return chakram.get(Prepper.foxboxManager.serviceListURL);
        })
        .then(function(listResponse) {
          expect(listResponse).to.have.status(200);
          var isFound = listResponse.body.some(
            entry => entry.id === 'thinkerbell/Second Recipe');
          expect(isFound).to.be.true;
          isFound = listResponse.body.some(
            entry => entry.id === 'thinkerbell/Third Recipe');
          expect(isFound).to.be.true;
        });
      });

    after(function(){
      var promises = ['First', 'Second', 'Third'].map(number =>
        chakram.put(Prepper.foxboxManager.setterURL, {'select':
      {'id': 'thinkerbell/' + number + ' Recipe/remove', 'feature': 'thinkerbell/remove-rule-id'},'value':null}));
      return Promise.all(promises);
    });
  });

  describe('remove recipes', function(){

    before(function(){

      var promises = ['First Recipe', 'Second Recipe', 'Third Recipe']
      .map(number =>
        chakram.put(Prepper.foxboxManager.setterURL,
          generateThinkerbellNewRecipePayload(number)));
      return Promise.all(promises);
    });

    it('Remove single recipe',function(){
      return chakram.put(Prepper.foxboxManager.setterURL,
        generateThinkerbellDeletionPayload('First Recipe'))
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse.body['thinkerbell/First Recipe/remove'])
        .equals(null);
        return chakram.get(Prepper.foxboxManager.serviceListURL);
      })
      .then(function(listResponse) {
        expect(listResponse).to.have.status(200);
        var isFound = listResponse.body.some(
          entry => entry.id === 'thinkerbell/First Recipe');
        expect(isFound).to.be.false;
      });
    });

    it('Remove all remaining recipes one by one',function(){
      return chakram.put(Prepper.foxboxManager.setterURL,
        generateThinkerbellDeletionPayload('Second Recipe'))
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse.body['thinkerbell/Second Recipe/remove'])
        .equals(null);
        return chakram.put(Prepper.foxboxManager.setterURL,
          generateThinkerbellDeletionPayload('Third Recipe'));
      })
      .then(function(cmdResponse) {
        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse.body['thinkerbell/Third Recipe/remove'])
        .equals(null);
        return chakram.get(Prepper.foxboxManager.serviceListURL);
      })
      .then(function(listResponse) {
        expect(listResponse).to.have.status(200);
        var isFound = listResponse.body.some(
          entry => entry.id === 'thinkerbell/Second Recipe');
        expect(isFound).to.be.false;
        isFound = listResponse.body.some(
          entry => entry.id === 'thinkerbell/Third Recipe');
        expect(isFound).to.be.false;
      });
    });
  });

  describe('get recipe details', function() {
    const recipeName = 'Recipe';

    before(function() {
      return chakram.put(
        Prepper.foxboxManager.setterURL,
        generateThinkerbellNewRecipePayload(recipeName)
      );
    });

    it('Get recipe "enabled" status', function() {
      return chakram.put(
        Prepper.foxboxManager.getterURL,
        generateThinkerbellGetEnabledPayload(recipeName)
      )
      .then((cmdResponse) => {
        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse.body['thinkerbell/' + recipeName + '/is-rule-enabled'])
          .equals('On');
      });
    });

    it('Get recipe source', function() {
      return chakram.put(
        Prepper.foxboxManager.getterURL,
        generateThinkerbellSourcePayload(recipeName)
      )
      .then((cmdResponse) => {
        const jsonResponse = {};
        jsonResponse['thinkerbell/' + recipeName + '/source'] = cloneRecipe(recipeName);

        expect(cmdResponse).to.have.status(200);
        expect(cmdResponse).to.comprise.of.json(jsonResponse);
      });
    });
  });
});
