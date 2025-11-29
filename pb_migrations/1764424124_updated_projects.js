/// <reference path="../pb_data/types.d.ts" />
migrate((app) => {
  const collection = app.findCollectionByNameOrId("pbc_484305853")

  // update collection data
  unmarshal({
    "indexes": [
      "CREATE UNIQUE INDEX `idx_9S0czAVATX` ON `projects` (`name`)"
    ]
  }, collection)

  // add field
  collection.fields.addAt(1, new Field({
    "autogeneratePattern": "",
    "hidden": false,
    "id": "text1579384326",
    "max": 255,
    "min": 1,
    "name": "name",
    "pattern": "^[a-z0-9-]+$",
    "presentable": false,
    "primaryKey": false,
    "required": true,
    "system": false,
    "type": "text"
  }))

  // add field
  collection.fields.addAt(2, new Field({
    "hidden": false,
    "id": "select1542800728",
    "maxSelect": 1,
    "name": "field",
    "presentable": false,
    "required": true,
    "system": false,
    "type": "select",
    "values": [
      "Stopped",
      "Running",
      "Deployed",
      "Failed"
    ]
  }))

  // add field
  collection.fields.addAt(3, new Field({
    "hidden": false,
    "id": "select1368277760",
    "maxSelect": 1,
    "name": "visibility",
    "presentable": false,
    "required": true,
    "system": false,
    "type": "select",
    "values": [
      "Public",
      "Private"
    ]
  }))

  return app.save(collection)
}, (app) => {
  const collection = app.findCollectionByNameOrId("pbc_484305853")

  // update collection data
  unmarshal({
    "indexes": []
  }, collection)

  // remove field
  collection.fields.removeById("text1579384326")

  // remove field
  collection.fields.removeById("select1542800728")

  // remove field
  collection.fields.removeById("select1368277760")

  return app.save(collection)
})
