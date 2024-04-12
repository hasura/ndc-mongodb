# MongoDB fixtures in the form of docker volume mounting strings
{
  all-fixtures = "${toString ./..}/fixtures/mongodb:/docker-entrypoint-initdb.d:ro";
  chinook = "${toString ./..}/fixtures/mongodb/chinook:/docker-entrypoint-initdb.d:ro";
}
