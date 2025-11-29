/// <reference path="../pb_data/types.d.ts" />
migrate((app) => {
  const collection = app.findCollectionByNameOrId("pbc_484305853")

  // update collection data
  unmarshal({
    "deleteRule": " @request.auth.id != \"\" && (owner_user.id = @request.auth.id || owner_team.members.id ?= @request.auth.id)",
    "listRule": "@request.auth.id != \"\" && (owner_user.id = @request.auth.id || owner_team.members.id ?= @request.auth.id)",
    "updateRule": "@request.auth.id != \"\" && (owner_user.id = @request.auth.id || owner_team.members.id ?= @request.auth.id)",
    "viewRule": "@request.auth.id != \"\" && (owner_user.id = @request.auth.id || owner_team.members.id ?= @request.auth.id)"
  }, collection)

  // add field
  collection.fields.addAt(4, new Field({
    "cascadeDelete": false,
    "collectionId": "_pb_users_auth_",
    "hidden": false,
    "id": "relation456504793",
    "maxSelect": 1,
    "minSelect": 0,
    "name": "owner_user",
    "presentable": false,
    "required": false,
    "system": false,
    "type": "relation"
  }))

  // add field
  collection.fields.addAt(5, new Field({
    "cascadeDelete": false,
    "collectionId": "pbc_1568971955",
    "hidden": false,
    "id": "relation1380369807",
    "maxSelect": 1,
    "minSelect": 0,
    "name": "owner_team",
    "presentable": false,
    "required": false,
    "system": false,
    "type": "relation"
  }))

  return app.save(collection)
}, (app) => {
  const collection = app.findCollectionByNameOrId("pbc_484305853")

  // update collection data
  unmarshal({
    "deleteRule": "@request.auth.id != \"\"",
    "listRule": "@request.auth.id != \"\"",
    "updateRule": "@request.auth.id != \"\"",
    "viewRule": "@request.auth.id != \"\""
  }, collection)

  // remove field
  collection.fields.removeById("relation456504793")

  // remove field
  collection.fields.removeById("relation1380369807")

  return app.save(collection)
})
