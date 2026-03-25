package openapi

// Spec is a minimal OpenAPI 3.x / Swagger 2.x document.
type Spec struct {
	OpenAPI string              `json:"openapi" yaml:"openapi"` // "3.x.x"
	Swagger string              `json:"swagger" yaml:"swagger"` // "2.x" fallback
	Info    Info                `json:"info"    yaml:"info"`
	Servers []Server            `json:"servers" yaml:"servers"`
	Paths   map[string]PathItem `json:"paths"   yaml:"paths"`
}

type Info struct {
	Title string `json:"title" yaml:"title"`
}

type Server struct {
	URL string `json:"url" yaml:"url"`
}

type PathItem struct {
	Get     *Operation `json:"get"     yaml:"get"`
	Post    *Operation `json:"post"    yaml:"post"`
	Put     *Operation `json:"put"     yaml:"put"`
	Patch   *Operation `json:"patch"   yaml:"patch"`
	Delete  *Operation `json:"delete"  yaml:"delete"`
	Head    *Operation `json:"head"    yaml:"head"`
	Options *Operation `json:"options" yaml:"options"`
}

type Operation struct {
	Summary     string      `json:"summary"     yaml:"summary"`
	Tags        []string    `json:"tags"        yaml:"tags"`
	Parameters  []Parameter `json:"parameters"  yaml:"parameters"`
	RequestBody *ReqBody    `json:"requestBody" yaml:"requestBody"`
}

type Parameter struct {
	Name string `json:"name" yaml:"name"`
	In   string `json:"in"   yaml:"in"`
}

type ReqBody struct {
	Content map[string]MediaType `json:"content" yaml:"content"`
}

type MediaType struct{}
