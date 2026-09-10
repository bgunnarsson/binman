package postmanfile

import "encoding/json"

// Parse unmarshals a Postman collection JSON file.
func Parse(data []byte) (*Collection, error) {
	var c Collection
	if err := json.Unmarshal(data, &c); err != nil {
		return nil, err
	}
	return &c, nil
}
