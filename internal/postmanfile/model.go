package postmanfile

import "encoding/json"

// Collection is the top-level Postman collection v2.1 structure.
type Collection struct {
	Info      Info       `json:"info"`
	Items     []Item     `json:"item"`
	Variables []Variable `json:"variable"`
}

// Variable is a collection-level variable definition.
type Variable struct {
	Key   string `json:"key"`
	Value string `json:"value"`
}

// Vars returns the collection variables as a key→value map.
func (c *Collection) Vars() map[string]string {
	if len(c.Variables) == 0 {
		return nil
	}
	m := make(map[string]string, len(c.Variables))
	for _, v := range c.Variables {
		m[v.Key] = v.Value
	}
	return m
}

type Info struct {
	Name string `json:"name"`
}

// Item is either a folder (has Items) or a request (has Request).
type Item struct {
	Name    string  `json:"name"`
	Request *PMReq  `json:"request"`
	Items   []Item  `json:"item"`
}

// PMReq is the request object inside an Item.
type PMReq struct {
	Method string     `json:"method"`
	URL    PostmanURL `json:"url"`
	Header []Header   `json:"header"`
	Body   *Body      `json:"body"`
}

// PostmanURL handles both string and object URL forms.
type PostmanURL struct {
	Raw string
}

func (u *PostmanURL) UnmarshalJSON(data []byte) error {
	// Try plain string first
	var s string
	if err := json.Unmarshal(data, &s); err == nil {
		u.Raw = s
		return nil
	}
	// Object form: {"raw": "..."}
	var obj struct {
		Raw string `json:"raw"`
	}
	if err := json.Unmarshal(data, &obj); err != nil {
		return err
	}
	u.Raw = obj.Raw
	return nil
}

type Header struct {
	Key      string `json:"key"`
	Value    string `json:"value"`
	Disabled bool   `json:"disabled"`
}

type Body struct {
	Mode string `json:"mode"`
	Raw  string `json:"raw"`
}
