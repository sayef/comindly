package com.acme.lib;

public class Repo extends Base implements Store {
    private Db db = new Db();

    public Item load(String id) {
        return db.query(id);
    }
}
